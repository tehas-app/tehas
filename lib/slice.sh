# shellcheck shell=bash
# The one reader of tehas.slice, the systemd slice every sandbox container
# runs in. Sourced by the commands that start a container and by the
# dispatcher. Never by lib/config.sh or a login shell: it calls systemctl and
# docker, and loading settings must not.
#
# Every function here only reads. Nothing is written, started or changed, and
# both outside calls run under "timeout 10" so a silent one cannot block a
# poll. A fault is never read as "no limit" or "an empty slice": the answer is
# then "not ok" or "no room".
#
#   tehas_slice_ok       is the slice on this host usable? One systemctl call,
#                        one docker call, one file read. Returns 0 and sets
#                          TEHAS_SLICE_MAX         the slice's MemoryMax, bytes
#                          TEHAS_SLICE_ACTIVE      "active", or another state
#                          TEHAS_SLICE_CEILING_MB  what one container may use:
#                                                  a quarter of the max, MiB
#                        or returns 1 and sets TEHAS_SLICE_WHY to one line a
#                        person can act on. A command starts with
#                          tehas_slice_ok || { echo "tehas x: $TEHAS_SLICE_WHY" >&2; exit 78; }
#   tehas_slice_limits   after tehas_slice_ok. No call, only arithmetic. Fills
#                        the array TEHAS_SLICE_LIMITS with exactly twelve
#                        words for docker run:
#                          --cgroup-parent tehas.slice --memory <n>m
#                          --memory-swap <n>m --memory-reservation <reserve>
#                          --pids-limit 4096 --cpu-shares 1024
#                        and returns 0, or leaves it empty and returns 1. The
#                        caller checks for twelve words and "tehas.slice" as
#                        the second before it starts anything.
#   tehas_slice_room <count> [<cid file>...]
#                        after tehas_slice_ok. How many more containers of one
#                        reserve fit. <count> is the cycles that hold a lock;
#                        each cid file is what "docker run --cidfile" wrote for
#                        one of them. Prints one line:
#                          <room> <committed> <max> <reserve> [<why>]
#                        The three in the middle are bytes. <why> is absent
#                        when there is room, else one word:
#                          room        the slice is promised to the full
#                          pressure    full avg60 of the slice is above 10
#                          unreadable  something could not be read; room is 0
#                        Always returns 0 and always prints the first four as
#                        numbers.
#   tehas_slice_pressure print "full avg60" of the slice's memory pressure, as
#                        the kernel writes it (0.00). Returns 1 with nothing
#                        printed when it cannot be read.
#
# Settings read: TEHAS_CYCLE_RESERVE (lib/config.sh gives it a default and
# checks it) and TEHAS_CGROUP_ROOT, the tests' seam for /sys/fs/cgroup.

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
  local out line path="" max="" active="" driver="" version="" ctl="" c reserve ceiling need
  local root=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup}
  TEHAS_SLICE_WHY="" TEHAS_SLICE_MAX="" TEHAS_SLICE_ACTIVE="" TEHAS_SLICE_CEILING_MB=""

  if ! out=$(timeout 10 systemctl show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax 2>/dev/null </dev/null) ||
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

  if ! out=$(timeout 10 docker info --format '{{.CgroupDriver}} {{.CgroupVersion}}' 2>/dev/null </dev/null); then
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

  TEHAS_SLICE_MAX=$max
  TEHAS_SLICE_ACTIVE=$active
  TEHAS_SLICE_CEILING_MB=$ceiling
  return 0
}

# shellcheck disable=SC2034  # the caller reads the array
tehas_slice_limits() {
  local mb=${TEHAS_SLICE_CEILING_MB:-}
  TEHAS_SLICE_LIMITS=()
  _tehas_slice_num "$mb" && [ "$mb" -gt 0 ] || return 1
  _tehas_slice_bytes "${TEHAS_CYCLE_RESERVE:-}" >/dev/null || return 1
  TEHAS_SLICE_LIMITS=(--cgroup-parent tehas.slice --memory "${mb}m" --memory-swap "${mb}m"
    --memory-reservation "$TEHAS_CYCLE_RESERVE" --pids-limit 4096 --cpu-shares 1024)
}

tehas_slice_pressure() {
  local file=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup}/tehas.slice/memory.pressure line
  [ -r "$file" ] || return 1
  while IFS= read -r line; do
    if [[ $line =~ ^full\ .*avg60=([0-9]+(\.[0-9]+)?)(\ |$) ]]; then
      printf '%s\n' "${BASH_REMATCH[1]}"
      return 0
    fi
  done <"$file" 2>/dev/null
  return 1
}

# What one child of the slice is charged with, in bytes: what it holds that
# cannot be reclaimed, or its floor, whichever is more. File cache and
# reclaimable slab can be reclaimed, so they are left out. A floor of "max"
# reads as one reserve. Fails when a file or a number is missing.
_tehas_slice_charge() { # <scope directory> <reserve in bytes>
  local dir=$1 reserve=$2 key value anon="" shmem="" kernel="" slab="" low="" used
  [ -r "$dir/memory.stat" ] && [ -r "$dir/memory.low" ] || return 1
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
  for value in "$anon" "$shmem" "$kernel" "$slab" "$low"; do
    _tehas_slice_num "$value" || return 1
  done
  used=$((anon + shmem + kernel - slab))
  echo $((used > low ? used : low))
}

tehas_slice_room() {
  local count=${1:-} root=${TEHAS_CGROUP_ROOT:-/sys/fs/cgroup} max=${TEHAS_SLICE_MAX:-} reserve
  local dir child charge file id names=" " committed=0 scoped=0 room pressure why=""
  [ "$#" -eq 0 ] || shift
  reserve=$(_tehas_slice_bytes "${TEHAS_CYCLE_RESERVE:-}") || reserve=0
  _tehas_slice_num "$max" || max=0
  if [ "$max" = 0 ] || [ "$reserve" = 0 ] || ! _tehas_slice_num "$count"; then
    echo "0 0 $max $reserve unreadable"
    return 0
  fi

  dir=$root/tehas.slice
  if [ ! -d "$dir" ]; then
    # systemd removes the directory of a slice that is not active. A cycle
    # holds its lock also while it has no container in the slice (the clone,
    # the end), so each one is promised a reserve and nothing is read. An
    # active slice with no directory is wrong.
    if [ "${TEHAS_SLICE_ACTIVE:-}" = active ]; then
      echo "0 0 $max $reserve unreadable"
      return 0
    fi
    committed=$((count * reserve))
    room=0
    [ "$committed" -ge "$max" ] || room=$(((max - committed) / reserve))
    [ "$room" -gt 0 ] || why=" room"
    echo "$room $committed $max $reserve$why"
    return 0
  fi

  for child in "$dir"/*.scope; do
    [ -d "$child" ] || continue
    if ! charge=$(_tehas_slice_charge "$child" "$reserve"); then
      # A container that ended while it was read is gone, not unreadable.
      [ -d "$child" ] || continue
      echo "0 0 $max $reserve unreadable"
      return 0
    fi
    committed=$((committed + charge))
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
  [ "$count" -le "$scoped" ] || committed=$((committed + (count - scoped) * reserve))

  if ! pressure=$(tehas_slice_pressure); then
    echo "0 $committed $max $reserve unreadable"
    return 0
  fi
  room=$(((max - committed) / reserve))
  [ "$room" -gt 0 ] || { room=0 why=room; }
  # Above 10 means the whole slice stood still for more than a tenth of the
  # last minute. Compared in hundredths, with no call to awk.
  if [[ $pressure =~ ^([0-9]+)(\.([0-9]{1,2}))? ]]; then
    id=${BASH_REMATCH[3]:-0}
    [ "${#id}" = 2 ] || id=${id}0
    if [ $((10#${BASH_REMATCH[1]} * 100 + 10#$id)) -gt 1000 ]; then room=0 why=pressure; fi
  fi
  echo "$room $committed $max $reserve${why:+ $why}"
}
