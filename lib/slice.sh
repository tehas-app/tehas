# shellcheck shell=bash
# The one reader of tehas.slice, the systemd slice every sandbox container
# runs in. Sourced by the commands that start a container and by the
# dispatcher. Never by lib/config.sh or a login shell: it calls systemctl and
# docker, and loading settings must not.
#
# Every function here only reads. Nothing is written, started or changed, and
# both outside calls run under "timeout 10" (TEHAS_SLICE_WAIT, for tests) so a
# silent one cannot block a poll. A fault is never read as "no limit" or "an empty slice": the answer is
# then "not ok" or "no room".
#
#   tehas_slice_ok       is the slice on this host usable? One systemctl call,
#                        one docker call, one file read. Returns 0 and sets
#                          TEHAS_SLICE_MAX         the slice's MemoryMax, bytes
#                          TEHAS_SLICE_ACTIVE      "active", or another state
#                          TEHAS_SLICE_CEILING_MB  what one container may use:
#                                                  a quarter of the max, MiB
#                          TEHAS_SLICE_CPUS        the host's CPUs, from nproc
#                        or returns 1 and sets TEHAS_SLICE_WHY to one line a
#                        person can act on. A command starts with
#                          tehas_slice_ok || { echo "tehas x: $TEHAS_SLICE_WHY" >&2; exit 78; }
#   tehas_slice_limits   after tehas_slice_ok. No call, only arithmetic. Fills
#                        the array TEHAS_SLICE_LIMITS with exactly fourteen
#                        words for docker run:
#                          --cgroup-parent tehas.slice --memory <n>m
#                          --memory-swap <n>m --memory-reservation <reserve>
#                          --cpus <c> --pids-limit <p> --cpu-shares 1024
#                        and returns 0, or leaves it empty and returns 1. The
#                        caller checks for fourteen words and "tehas.slice" as
#                        the second before it starts anything. <c> and <p>
#                        are what tehas_slice_cpus prints.
#   tehas_slice_cpus     after tehas_slice_ok. No call. Prints the CPU quota
#                        and the pids limit of one container: "<c> <p>".
#                        <c> is the host's CPUs less one, never below one
#                        CPU, as docker writes it (15.00). <p> is 1024 for
#                        each CPU of the container's share, rounded up, from
#                        2048 to 4096. The share is <c> over the cycles that
#                        may run at once (TEHAS_TICKET_CONCURRENCY, and
#                        TEHAS_REFRESH_SLOTS more with
#                        TEHAS_REFRESH_CONFLICTS=1, at most the four the
#                        slice holds), never below one CPU.
#                        Returns 1 with nothing printed without a probe or
#                        with a concurrency that is not a number.
#   tehas_slice_verify   after tehas_slice_limits. No call. Prints how many
#                        runs of the product's gate one container runs at
#                        once: one for each two whole CPUs of its share and
#                        each 2 GiB of its memory ceiling, the lower, at least
#                        one. The rule pi-rukas applies to a host, applied to
#                        the container. Returns 1 with nothing printed when
#                        TEHAS_SLICE_LIMITS has no quota or ceiling.
#   tehas_slice_room <count> [<cid file>...]
#                        after tehas_slice_ok. How many more containers of one
#                        reserve fit, and never more than leave the slice with
#                        as many containers as their ceilings fit in its max
#                        (four). <count> is the cycles that hold a lock;
#                        each cid file is what "docker run --cidfile" wrote for
#                        one of them. Prints one line:
#                          <room> <committed> <max> <reserve> [<why>]
#                        The three in the middle are bytes. <why> is absent
#                        when there is room, else one word:
#                          room        the slice is promised to the full
#                          full        the slice has as many containers as
#                                      their ceilings fit
#                          pressure    full avg60 of the slice is above 10
#                                      and more than half the max is promised
#                          unreadable  something could not be read; room is 0
#                        After unreadable, the rest of the line names the file
#                        and what was missing in it.
#                        Always returns 0 and always prints the first four as
#                        numbers.
#   tehas_slice_pressure print "full avg60" of the slice's memory pressure, as
#                        the kernel writes it (0.00). Returns 1 with nothing
#                        printed when it cannot be read, and sets
#                        TEHAS_SLICE_WHY to the file and what was missing.
#
# Settings read: TEHAS_CYCLE_RESERVE, TEHAS_TICKET_CONCURRENCY,
# TEHAS_REFRESH_CONFLICTS and TEHAS_REFRESH_SLOTS
# (lib/config.sh gives them defaults and checks them) and TEHAS_CGROUP_ROOT,
# the tests' seam for /sys/fs/cgroup.

_tehas_slice_num() { [[ ${1:-} =~ ^(0|[1-9][0-9]{0,17})$ ]]; }

# A Docker size (6m, 2g, a bare number of bytes) in bytes; fails on anything
# else and on zero.
_tehas_slice_bytes() {
  local n
  [[ ${1:-} =~ ^([1-9][0-9]{0,11})([bkmgBKMG]?)$ ]] || return 1
  n=${BASH_REMATCH[1]}
  case "${BASH_REMATCH[2]}" in
    '' | b | B) echo "$n" ;;
    *)
      # Nine digits of gigabytes still fit a 64-bit number.
      [ "${#n}" -le 9 ] || return 1
      case "${BASH_REMATCH[2]}" in
        k | K) echo $((n * 1024)) ;;
        m | M) echo $((n * 1048576)) ;;
        *) echo $((n * 1073741824)) ;;
      esac
      ;;
  esac
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_slice_ok() {
  local out line path="" max="" active="" driver="" version="" ctl="" c reserve ceiling need cpus
  local root=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup}
  TEHAS_SLICE_WHY="" TEHAS_SLICE_MAX="" TEHAS_SLICE_ACTIVE="" TEHAS_SLICE_CEILING_MB="" TEHAS_SLICE_CPUS=""

  if ! out=$(timeout "${TEHAS_SLICE_WAIT:-10}" systemctl show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax 2>/dev/null </dev/null) ||
    [ -z "$out" ]; then
    TEHAS_SLICE_WHY="systemctl gave no answer"
    return 1
  fi
  while IFS= read -r line; do
    case "$line" in
      FragmentPath=*) path=${line#*=} ;;
      ActiveState=*) active=${line#*=} ;;
      MemoryMax=*) max=${line#*=} ;;
    esac
  done <<<"$out"
  # A slice that is only a name has no file and no limit. It is never used.
  if [ -z "$path" ]; then
    TEHAS_SLICE_WHY="no slice file on this host (sudo ./bootstrap.sh slice)"
    return 1
  fi
  if ! _tehas_slice_num "$max"; then
    TEHAS_SLICE_WHY="tehas.slice has no memory limit (sudo ./bootstrap.sh slice)"
    return 1
  fi

  if ! out=$(timeout "${TEHAS_SLICE_WAIT:-10}" docker info --format '{{.CgroupDriver}} {{.CgroupVersion}}' 2>/dev/null </dev/null); then
    out=""
  fi
  read -r driver version <<<"$out" || true
  if [ -z "$driver" ] || [ -z "$version" ]; then
    TEHAS_SLICE_WHY="docker gave no answer"
    return 1
  fi
  if [ "$driver $version" != "systemd 2" ]; then
    TEHAS_SLICE_WHY="Docker runs containers with the $driver cgroup driver on cgroup v$version, tehas needs systemd on v2"
    return 1
  fi

  if [ -r "$root/cgroup.controllers" ]; then
    read -r ctl <"$root/cgroup.controllers" || true
  fi
  for c in cpu memory pids; do
    case " $ctl " in
      *" $c "*) ;;
      *)
        TEHAS_SLICE_WHY="$root/cgroup.controllers does not list cpu, memory and pids"
        return 1
        ;;
    esac
  done

  if ! reserve=$(_tehas_slice_bytes "${TEHAS_CYCLE_RESERVE:-}"); then
    TEHAS_SLICE_WHY="TEHAS_CYCLE_RESERVE=${TEHAS_CYCLE_RESERVE:-} is not a size"
    return 1
  fi
  ceiling=$((max / 4 / 1048576))
  if [ "$reserve" -gt $((ceiling * 1048576)) ]; then
    # The slice is three quarters of the host and a container a quarter of the
    # slice, so the reserve needs a host of 16/3 times its size.
    need=$((reserve * 16 / 3))
    if [ "$need" -ge 1073741824 ]; then
      need="$(((need + 536870912) / 1073741824)) GiB"
    else
      need="$((need / 1048576 + 1)) MiB"
    fi
    TEHAS_SLICE_WHY="TEHAS_CYCLE_RESERVE=$TEHAS_CYCLE_RESERVE is more than a container may use here (${ceiling}m, a quarter of tehas.slice's MemoryMax); the host needs about $need of memory for this reserve"
    return 1
  fi

  cpus=$(nproc 2>/dev/null) || cpus=""
  if ! _tehas_slice_num "$cpus" || [ "$cpus" = 0 ]; then
    TEHAS_SLICE_WHY="nproc gave no count of the host's CPUs"
    return 1
  fi

  TEHAS_SLICE_MAX=$max
  TEHAS_SLICE_ACTIVE=$active
  TEHAS_SLICE_CEILING_MB=$ceiling
  TEHAS_SLICE_CPUS=$cpus
  return 0
}

# One cycle's share of the host's CPUs less one, in hundredths, never below
# one CPU: the CPUs it gets when every cycle side by side is busy.
_tehas_slice_share() {
  local cpus=${TEHAS_SLICE_CPUS:-} cycles=${TEHAS_TICKET_CONCURRENCY:-} spare share
  _tehas_slice_num "$cpus" && [ "$cpus" -gt 0 ] || return 1
  _tehas_slice_num "$cycles" && [ "$cycles" -gt 0 ] || return 1
  [ "${TEHAS_REFRESH_CONFLICTS:-}" != 1 ] || ! _tehas_slice_num "${TEHAS_REFRESH_SLOTS:-}" ||
    cycles=$((cycles + TEHAS_REFRESH_SLOTS))
  [ "$cycles" -le 4 ] || cycles=4
  spare=$((cpus > 1 ? cpus - 1 : 1))
  share=$((spare * 100 / cycles))
  [ "$share" -ge 100 ] || share=100
  echo "$share"
}

# The quota is a ceiling, not a share that is held: a container may use every
# CPU but one, and the weight splits them when several are busy. A gate run
# alone gets the whole host, not a fixed part of it. One CPU is left to the
# host for its own services and the model backends.
#
# The pids limit follows the share, not the quota: a container's processes are
# mostly what its gate runs side by side, and a gate sized by its quota on a
# busy host gets its share of CPU, not more processes' worth of it. 2048 at
# least, for the agents, their tools and git around the gate.
tehas_slice_cpus() {
  local cpus=${TEHAS_SLICE_CPUS:-} share quota pids
  share=$(_tehas_slice_share) || return 1
  quota=$((cpus > 1 ? (cpus - 1) * 100 : 100))
  pids=$(((share + 99) / 100))
  pids=$((pids * 1024))
  [ "$pids" -ge 2048 ] || pids=2048
  [ "$pids" -le 4096 ] || pids=4096
  printf '%d.%02d %d\n' $((quota / 100)) $((quota % 100)) "$pids"
}

# shellcheck disable=SC2034  # the caller reads the array
tehas_slice_limits() {
  local mb=${TEHAS_SLICE_CEILING_MB:-} cpu quota pids
  TEHAS_SLICE_LIMITS=()
  _tehas_slice_num "$mb" && [ "$mb" -gt 0 ] || return 1
  _tehas_slice_bytes "${TEHAS_CYCLE_RESERVE:-}" >/dev/null || return 1
  cpu=$(tehas_slice_cpus) || return 1
  read -r quota pids <<<"$cpu"
  TEHAS_SLICE_LIMITS=(--cgroup-parent tehas.slice --memory "${mb}m" --memory-swap "${mb}m"
    --memory-reservation "$TEHAS_CYCLE_RESERVE" --cpus "$quota" --pids-limit "$pids" --cpu-shares 1024)
}

tehas_slice_verify() {
  local i cpus="" mb="" n share
  for ((i = 0; i + 1 < ${#TEHAS_SLICE_LIMITS[@]}; i++)); do
    case "${TEHAS_SLICE_LIMITS[i]}" in
      --cpus) cpus=${TEHAS_SLICE_LIMITS[i + 1]%%.*} ;;
      --memory) mb=${TEHAS_SLICE_LIMITS[i + 1]%m} ;;
    esac
  done
  _tehas_slice_num "$cpus" && _tehas_slice_num "$mb" || return 1
  # Each run is sized for the share: the quota is the whole host less one, and
  # every cycle side by side may be running its gate.
  if share=$(_tehas_slice_share) && [ $((share / 100)) -lt "$cpus" ]; then
    cpus=$((share / 100))
  fi
  n=$((cpus / 2))
  [ $((mb / 2048)) -ge "$n" ] || n=$((mb / 2048))
  [ "$n" -ge 1 ] || n=1
  printf '%d\n' "$n"
}

tehas_slice_pressure() {
  local file=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup}/tehas.slice/memory.pressure line
  TEHAS_SLICE_WHY=""
  if [ ! -r "$file" ]; then
    TEHAS_SLICE_WHY="$file cannot be read"
    return 1
  fi
  while IFS= read -r line; do
    if [[ $line =~ ^full\ .*avg60=([0-9]+(\.[0-9]+)?)(\ |$) ]]; then
      printf '%s\n' "${BASH_REMATCH[1]}"
      return 0
    fi
  done <"$file" 2>/dev/null
  TEHAS_SLICE_WHY="$file has no full avg60"
  return 1
}

# What one child of the slice is charged with, in bytes: what it holds that
# cannot be reclaimed, or its floor, whichever is more. File cache and
# reclaimable slab can be reclaimed, so they are left out. A floor of "max"
# reads as one reserve. Fails when a file or a number is missing, and then
# prints the file and the key instead.
#
# A kernel older than the "kernel" line (Ubuntu 22.04's 5.15 has none) would
# otherwise hold every poll for ever. Without it kernel memory is left out:
# anon and shmem are counted and slab is not subtracted, so the child counts
# a little low, and its floor still counts in full.
_tehas_slice_charge() { # <scope directory> <reserve in bytes>
  local dir=$1 reserve=$2 key value anon="" shmem="" kernel="" slab="" low="" used
  for key in memory.stat memory.low; do
    [ -r "$dir/$key" ] || { echo "$dir/$key cannot be read"; return 1; }
  done
  while read -r key value; do
    case "$key" in
      anon) anon=$value ;;
      shmem) shmem=$value ;;
      kernel) kernel=$value ;;
      slab_reclaimable) slab=$value ;;
    esac
  done <"$dir/memory.stat" 2>/dev/null
  read -r low <"$dir/memory.low" 2>/dev/null || true
  [ "$low" != max ] || low=$reserve
  if [ -z "$kernel" ]; then kernel=0 slab=0; fi
  for key in anon shmem kernel slab_reclaimable; do
    case "$key" in
      anon) value=$anon ;;
      shmem) value=$shmem ;;
      kernel) value=$kernel ;;
      *) value=$slab ;;
    esac
    _tehas_slice_num "$value" || { echo "$dir/memory.stat has no number for $key"; return 1; }
  done
  _tehas_slice_num "$low" || { echo "$dir/memory.low is not a number"; return 1; }
  used=$((anon + shmem + kernel - slab))
  echo $((used > low ? used : low))
}

# Sets room and why in the caller: the reserves that fit in what is not
# promised, capped so the containers' ceilings never sum to more than the
# max. Room by use alone lets in cycles at their small first steps that may
# each grow to a ceiling later; then the whole slice reclaims and the kernel
# kills in the biggest. The cap does not depend on TEHAS_TICKET_CONCURRENCY,
# so a dev session or a planning turn counts against it too.
_tehas_slice_fit() { # <max> <reserve> <ceiling MiB> <committed> <containers>
  local slots=$(($1 / ($3 * 1048576)))
  room=0 why=""
  [ "$4" -ge "$1" ] || room=$((($1 - $4) / $2))
  if [ "$room" -le 0 ]; then
    room=0 why=room
  elif [ "$5" -ge "$slots" ]; then
    room=0 why=full
  elif [ "$room" -gt $((slots - $5)) ]; then
    room=$((slots - $5))
  fi
}

tehas_slice_room() {
  local count=${1:-} root=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup} max=${TEHAS_SLICE_MAX:-} reserve
  local dir child charge file id names=" " committed=0 scoped=0 room pressure why=""
  local ceiling=${TEHAS_SLICE_CEILING_MB:-} containers=0
  [ "$#" -eq 0 ] || shift
  reserve=$(_tehas_slice_bytes "${TEHAS_CYCLE_RESERVE:-}") || reserve=0
  _tehas_slice_num "$max" || max=0
  _tehas_slice_num "$ceiling" || ceiling=0
  if [ "$max" = 0 ] || [ "$reserve" = 0 ] || [ "$ceiling" = 0 ]; then
    echo "0 0 $max $reserve unreadable the slice was not probed"
    return 0
  fi
  if ! _tehas_slice_num "$count"; then
    echo "0 0 $max $reserve unreadable the count of cycles is not a number"
    return 0
  fi

  dir=$root/tehas.slice
  if [ ! -d "$dir" ]; then
    # systemd removes the directory of a slice that is not active. A cycle
    # holds its lock also while it has no container in the slice (the clone,
    # the end), so each one is promised a reserve and nothing is read. An
    # active slice with no directory is wrong.
    if [ "${TEHAS_SLICE_ACTIVE:-}" = active ]; then
      echo "0 0 $max $reserve unreadable tehas.slice is active and $dir is missing"
      return 0
    fi
    committed=$((count * reserve))
    _tehas_slice_fit "$max" "$reserve" "$ceiling" "$committed" "$count"
    echo "$room $committed $max $reserve${why:+ $why}"
    return 0
  fi

  for child in "$dir"/*.scope; do
    [ -d "$child" ] || continue
    if ! charge=$(_tehas_slice_charge "$child" "$reserve"); then
      # A container that ended while it was read is gone, not unreadable.
      [ -d "$child" ] || continue
      echo "0 0 $max $reserve unreadable $charge"
      return 0
    fi
    committed=$((committed + charge))
    containers=$((containers + 1))
    names+="${child##*/} "
  done

  # A cycle that holds its lock and has no container yet is promised one
  # reserve. The cid files tell which cycles are among the children already.
  for file in "$@"; do
    id=""
    if [ -r "$file" ]; then read -r id <"$file" 2>/dev/null || true; fi
    [[ $id =~ ^[0-9a-f]{64}$ ]] || continue
    case "$names" in *" docker-$id.scope "*) scoped=$((scoped + 1)) ;; esac
  done
  if [ "$count" -gt "$scoped" ]; then
    committed=$((committed + (count - scoped) * reserve))
    containers=$((containers + count - scoped))
  fi

  # The reason is set in the subshell, so it comes back on the same line.
  pressure=$(tehas_slice_pressure || echo "!$TEHAS_SLICE_WHY")
  if [[ $pressure == !* ]]; then
    echo "0 $committed $max $reserve unreadable ${pressure#!}"
    return 0
  fi
  _tehas_slice_fit "$max" "$reserve" "$ceiling" "$committed" "$containers"
  # Above 10 means the whole slice stood still for more than a tenth of the
  # last minute. Compared in hundredths, with no call to awk. A slice's
  # pressure counts every stall in it, also one container's at its own
  # ceiling, which another container does not change. So it holds only once
  # more than half the slice is promised: a container's ceiling is at most a
  # quarter, so no single one gets there by itself.
  if [ $((committed * 2)) -gt "$max" ] && [[ $pressure =~ ^([0-9]+)(\.([0-9]{1,2}))? ]]; then
    id=${BASH_REMATCH[3]:-0}
    [ "${#id}" = 2 ] || id=${id}0
    if [ $((10#${BASH_REMATCH[1]} * 100 + 10#$id)) -gt 1000 ]; then room=0 why=pressure; fi
  fi
  echo "$room $committed $max $reserve${why:+ $why}"
}
