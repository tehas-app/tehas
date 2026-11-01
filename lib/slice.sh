# shellcheck shell=bash
# The bash poll's reader of tehas.slice, the systemd slice every sandbox
# container runs in, and tehas improve's. The commands that start a container
# read it in console/core/src/slice.rs, which makes their limits too. Never
# sourced by lib/config.sh or a login shell: it calls systemctl and docker, and
# loading settings must not.
#
# Every function here only reads. Nothing is written, started or changed, and
# both outside calls run under "timeout 10" (TEHAS_SLICE_WAIT, for tests) so a
# silent one cannot block a poll. A fault is never read as "no limit" or "an
# empty slice": the answer is then "not ok" or "no room".
#
#   tehas_slice_ok       is the slice on this host usable? One systemctl call,
#                        one docker call, one file read. Returns 0 and sets
#                          TEHAS_SLICE_MAX         the slice's MemoryMax, bytes
#                          TEHAS_SLICE_ACTIVE      "active", or another state
#                          TEHAS_SLICE_CEILING_MB  what one container may use:
#                                                  max / TEHAS_SLICE_CONTAINERS, MiB
#                          TEHAS_SLICE_CPUS        the host's CPUs, from nproc
#                        or returns 1 and sets TEHAS_SLICE_WHY to one line a
#                        person can act on. A command starts with
#                          tehas_slice_ok || { echo "tehas x: $TEHAS_SLICE_WHY" >&2; exit 78; }
#   tehas_slice_room <count> [<cid file>...]
#                        after tehas_slice_ok. How many more containers of one
#                        reserve fit, never more than TEHAS_SLICE_CONTAINERS
#                        in the slice. <count> is the cycles that hold a lock;
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
#                        Always returns 0 and prints the first four as numbers.
#   tehas_slice_pressure print "full avg60" of the slice's memory pressure, as
#                        the kernel writes it (0.00). Returns 1 with nothing
#                        printed when it cannot be read, and sets
#                        TEHAS_SLICE_WHY to the file and what was missing.
#
# Settings read: TEHAS_CYCLE_RESERVE and TEHAS_SLICE_CONTAINERS (lib/config.sh
# gives them defaults and checks them) and TEHAS_CGROUP_ROOT, the tests' seam
# for /sys/fs/cgroup.

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
  local root=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup} n=${TEHAS_SLICE_CONTAINERS:-4}
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
  _tehas_slice_num "$n" && [ "$n" != 0 ] || { TEHAS_SLICE_WHY="TEHAS_SLICE_CONTAINERS=$n is not a count"; return 1; }
  ceiling=$((max / n / 1048576))
  if [ "$reserve" -gt $((ceiling * 1048576)) ]; then
    # The slice is three quarters of the host and a container 1/n of the
    # slice, so the reserve needs a host of 4n/3 times its size.
    need=$((reserve * 4 * n / 3))
    if [ "$need" -ge 1073741824 ]; then
      need="$(((need + 536870912) / 1073741824)) GiB"
    else
      need="$((need / 1048576 + 1)) MiB"
    fi
    TEHAS_SLICE_WHY="TEHAS_CYCLE_RESERVE=$TEHAS_CYCLE_RESERVE is more than a container may use here (${ceiling}m, tehas.slice's MemoryMax over TEHAS_SLICE_CONTAINERS=$n); the host needs about $need of memory for this reserve"
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
# reclaimable slab can be reclaimed, so they are left out. A floor of "max" reads
# as one reserve. Fails when a file or a number is missing, and then prints the
# file and the key instead. A scope the kernel has not filled in yet, a container
# that is starting, has no slab line: that reads as 0, so the slab counts as used
# and it holds rather than starting. A kernel older than the "kernel" line
# (Ubuntu 22.04's 5.15 has none) would otherwise hold every poll for ever. Without
# it kernel memory is left out: anon and shmem are counted and slab is not
# subtracted, so the child counts a little low, and its floor counts in full.
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
  [ -n "$kernel" ] || { kernel=0 slab=0; }; [ -n "$slab" ] || slab=0
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
  # more than half the slice is promised, which no ceiling is alone.
  if [ $((committed * 2)) -gt "$max" ] && [[ $pressure =~ ^([0-9]+)(\.([0-9]{1,2}))? ]]; then
    id=${BASH_REMATCH[3]:-0}
    [ "${#id}" = 2 ] || id=${id}0
    if [ $((10#${BASH_REMATCH[1]} * 100 + 10#$id)) -gt 1000 ]; then room=0 why=pressure; fi
  fi
  echo "$room $committed $max $reserve${why:+ $why}"
}
