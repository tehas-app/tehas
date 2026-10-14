#!/usr/bin/env bash
# Tests for lib/slice.sh: whether the slice on this host is usable, the limits
# a container is started with, and how much room the slice has left.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

GIB=1073741824
MIB=1048576
MAX=$((20 * GIB))
pre='. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/slice.sh"; tehas_load || exit 9; '
ok="$pre"'tehas_slice_ok; echo "rc=$? max=$TEHAS_SLICE_MAX active=$TEHAS_SLICE_ACTIVE ceiling=$TEHAS_SLICE_CEILING_MB why=[$TEHAS_SLICE_WHY]"'
a=$(slice_id a)
b=$(slice_id b)

# systemd <FragmentPath> <ActiveState> <MemoryMax>: what systemctl show answers.
systemd() { t_stub systemctl "printf '%s\\n' 'MemoryMax=$3' 'ActiveState=$2' 'FragmentPath=$1'"; }
# A host where everything is in place: the slice loaded from its file and
# active, Docker on the systemd driver and cgroup v2, an empty slice.
good() {
  rm -f "$T"/calls/*
  systemd /etc/systemd/system/tehas.slice active "$MAX"
  t_stub docker "echo 'systemd 2'"
  slice_state 0.00
  t_instance "${T_MINIMAL[@]}"
  T_ENV=(TEHAS_CGROUP_ROOT="$T_CGROUP")
}
# room <count> [<cid file>...]: what tehas_slice_room prints after a good probe.
room() { t_run "$pre"'tehas_slice_ok || echo "not ok: $TEHAS_SLICE_WHY"; tehas_slice_room '"$*"; }

# --- tehas_slice_ok ---------------------------------------------------------
t_case "ok passes with a file, a numeric MemoryMax, systemd on v2 and the controllers"
good
t_run "$ok"
t_eq "$t_out" "rc=0 max=$MAX active=active ceiling=5120 why=[]"

t_case "ok makes exactly one systemctl call and one docker call"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker)" "info --format {{.CgroupDriver}} {{.CgroupVersion}}"

t_case "an inactive slice answers from its file"
good
systemd /etc/systemd/system/tehas.slice inactive "$MAX"
t_run "$ok"
t_eq "$t_out" "rc=0 max=$MAX active=inactive ceiling=5120 why=[]"

t_case "the ceiling is a quarter of the max in MiB, rounded down"
good
systemd /etc/systemd/system/tehas.slice active $((MAX + 3 * MIB + 5))
t_run "$ok"
t_contains "$t_out" "rc=0 "
t_contains "$t_out" "ceiling=5120 "

t_case "a slice with no file is refused and names the root step"
good
systemd "" inactive infinity
t_run "$ok"
t_eq "$t_out" "rc=1 max= active= ceiling= why=[no slice file on this host (sudo ./bootstrap.sh slice)]"
t_eq "$(t_calls docker)" ""

t_case "a slice with no memory limit is refused"
good
systemd /etc/systemd/system/tehas.slice active infinity
t_run "$ok"
t_eq "$t_out" "rc=1 max= active= ceiling= why=[tehas.slice has no memory limit (sudo ./bootstrap.sh slice)]"

t_case "an answer without MemoryMax is no memory limit"
good
t_stub systemctl "echo FragmentPath=/etc/systemd/system/tehas.slice"
t_run "$ok"
t_contains "$t_out" "rc=1 "
t_contains "$t_out" "why=[tehas.slice has no memory limit"

t_case "a cgroupfs driver is refused and named"
good
t_stub docker "echo 'cgroupfs 2'"
t_run "$ok"
t_contains "$t_out" "rc=1 max= active= ceiling= "
t_contains "$t_out" "why=[Docker runs containers with the cgroupfs cgroup driver on cgroup v2, tehas needs systemd on v2]"

t_case "cgroup v1 is refused and named"
good
t_stub docker "echo 'systemd 1'"
t_run "$ok"
t_contains "$t_out" "rc=1 "
t_contains "$t_out" "why=[Docker runs containers with the systemd cgroup driver on cgroup v1, tehas needs systemd on v2]"

t_case "a systemctl that fails is no answer"
good
t_stub systemctl "echo FragmentPath=/etc/systemd/system/tehas.slice; echo MemoryMax=$MAX; exit 1"
t_run "$ok"
t_eq "$t_out" "rc=1 max= active= ceiling= why=[systemctl gave no answer]"

t_case "a systemctl that prints nothing is no answer"
good
t_stub systemctl
t_run "$ok"
t_contains "$t_out" "why=[systemctl gave no answer]"

t_case "a silent systemctl is no answer"
good
t_stub systemctl "sleep 20"
started=$SECONDS
t_run "$ok"
t_contains "$t_out" "rc=1 "
t_contains "$t_out" "why=[systemctl gave no answer]"
[ $((SECONDS - started)) -lt 16 ] || t_bad "the probe took $((SECONDS - started)) s"

t_case "a docker that fails is no answer"
good
t_stub docker "echo 'systemd 2'; exit 1"
t_run "$ok"
t_eq "$t_out" "rc=1 max= active= ceiling= why=[docker gave no answer]"

t_case "a docker that answers half is no answer"
good
t_stub docker "echo systemd"
t_run "$ok"
t_contains "$t_out" "why=[docker gave no answer]"

t_case "controllers without cpu are refused"
good
echo "cpuset io memory pids" >"$T_CGROUP/cgroup.controllers"
t_run "$ok"
t_eq "$t_out" "rc=1 max= active= ceiling= why=[$T_CGROUP/cgroup.controllers does not list cpu, memory and pids]"

t_case "a missing cgroup root is refused"
good
T_ENV=(TEHAS_CGROUP_ROOT="$T/nowhere")
t_run "$ok"
t_contains "$t_out" "rc=1 "
t_contains "$t_out" "why=[$T/nowhere/cgroup.controllers does not list cpu, memory and pids]"

t_case "a reserve above a quarter is refused and names both numbers and the smallest host"
good
systemd /etc/systemd/system/tehas.slice active $((6 * GIB))
t_run "$ok"
t_eq "$t_out" "rc=1 max= active= ceiling= why=[TEHAS_CYCLE_RESERVE=2g is more than a container may use here (1536m, a quarter of tehas.slice's MemoryMax); the host needs about 11 GiB of memory for this reserve]"

t_case "a reserve of exactly a quarter is accepted"
good
systemd /etc/systemd/system/tehas.slice active $((8 * GIB))
t_run "$ok"
t_contains "$t_out" "rc=0 "
t_contains "$t_out" "ceiling=2048 "

t_case "a failed probe leaves nothing from the one before it"
good
t_run "$pre"'tehas_slice_ok; rm "$TEHAS_CGROUP_ROOT/cgroup.controllers"; tehas_slice_ok; echo "rc=$? max=[$TEHAS_SLICE_MAX] ceiling=[$TEHAS_SLICE_CEILING_MB]"; tehas_slice_limits; echo "limits=$? ${#TEHAS_SLICE_LIMITS[@]}"'
t_eq "$t_out" "rc=1 max=[] ceiling=[]
limits=1 0"

# --- tehas_slice_limits -----------------------------------------------------
t_case "limits fills exactly the twelve words with a quarter of the max in MiB"
good
t_run "$pre"'tehas_slice_ok && tehas_slice_limits; echo "rc=$? n=${#TEHAS_SLICE_LIMITS[@]}"; printf "%s\n" "${TEHAS_SLICE_LIMITS[*]}"'
t_eq "$t_out" "rc=0 n=12
--cgroup-parent tehas.slice --memory 5120m --memory-swap 5120m --memory-reservation 2g --pids-limit 4096 --cpu-shares 1024"

t_case "limits makes no call"
t_eq "$(wc -l <"$T/calls/systemctl" | tr -d ' ') $(wc -l <"$T/calls/docker" | tr -d ' ')" "1 1"

t_case "limits carries the reserve as the instance wrote it"
good
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=512m
t_run "$pre"'tehas_slice_ok && tehas_slice_limits; echo "${TEHAS_SLICE_LIMITS[7]}"'
t_eq "$t_out" "512m"

t_case "limits without a probe is empty and fails"
good
t_run "$pre"'tehas_slice_limits; echo "rc=$? n=${#TEHAS_SLICE_LIMITS[@]}"'
t_eq "$t_out" "rc=1 n=0"
t_eq "$(t_calls systemctl)$(t_calls docker)" ""

# --- tehas_slice_room -------------------------------------------------------
t_case "an inactive slice with no directory and no cycle is empty"
good
systemd /etc/systemd/system/tehas.slice inactive "$MAX"
rm -rf "$T_CGROUP/tehas.slice"
room 0
t_eq "$t_out" "10 0 $MAX $((2 * GIB))"

t_case "an inactive slice with no directory and a cycle between containers promises its reserve"
room 1
t_eq "$t_out" "9 $((2 * GIB)) $MAX $((2 * GIB))"

t_case "an inactive slice with no directory and more cycles than fit has no room"
room 11
t_eq "$t_out" "0 $((22 * GIB)) $MAX $((2 * GIB)) room"

t_case "no directory with the slice active and a cycle running is unreadable"
good
rm -rf "$T_CGROUP/tehas.slice"
room 1
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable tehas.slice is active and $T_CGROUP/tehas.slice is missing"

t_case "no directory with the slice active is unreadable"
good
rm -rf "$T_CGROUP/tehas.slice"
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable tehas.slice is active and $T_CGROUP/tehas.slice is missing"

t_case "an empty slice has room for the max divided by the reserve"
good
room 0
t_eq "$t_out" "10 0 $MAX $((2 * GIB))"

t_case "two cycles at 500 MiB with a floor of 2 GiB count 2 GiB each"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
slice_child "$b" $((500 * MIB)) 0 $((2 * GIB))
echo "$a" >"$T/a.cid"
printf '%s' "$b" >"$T/b.cid"
room 2 '"$T/a.cid" "$T/b.cid"'
t_eq "$t_out" "8 $((4 * GIB)) $MAX $((2 * GIB))"

t_case "a child at 3 GiB counts 3 GiB"
slice_child "$a" $((3 * GIB)) 0 $((2 * GIB))
room 2 '"$T/a.cid" "$T/b.cid"'
t_eq "$t_out" "7 $((5 * GIB)) $MAX $((2 * GIB))"

t_case "a container that is no cycle counts too"
good
slice_child "$(slice_id c)" $((3 * GIB)) 0 $((2 * GIB))
room 0
t_eq "$t_out" "8 $((3 * GIB)) $MAX $((2 * GIB))"

t_case "a floor of max reads as the reserve, and no floor is the use"
good
slice_child "$a" $((100 * MIB)) 0 max
slice_child "$b" $((100 * MIB)) 0 0
room 0
t_eq "$t_out" "8 $((2 * GIB + 100 * MIB)) $MAX $((2 * GIB))"

t_case "a cid file with no child adds one reserve"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
echo "$a" >"$T/a.cid"
echo "$b" >"$T/b.cid"
room 2 '"$T/a.cid" "$T/b.cid"'
t_eq "$t_out" "8 $((4 * GIB)) $MAX $((2 * GIB))"

t_case "a cycle counted by count with no cid file adds one reserve"
room 3 '"$T/a.cid" "$T/missing.cid"'
t_eq "$t_out" "7 $((6 * GIB)) $MAX $((2 * GIB))"

t_case "more children than cycles add nothing for the count"
room 0 '"$T/a.cid"'
t_eq "$t_out" "9 $((2 * GIB)) $MAX $((2 * GIB))"

t_case "a cid file that is not 64 hex characters is ignored"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
slice_child "short" $((500 * MIB)) 0 $((2 * GIB))
echo "short" >"$T/a.cid"
echo "../docker-$a" >"$T/b.cid"
room 1 '"$T/a.cid" "$T/b.cid"'
t_eq "$t_out" "7 $((6 * GIB)) $MAX $((2 * GIB))"

t_case "reclaimable slab does not count"
good
slice_child "$a" $((3 * GIB)) $((6 * GIB)) $((2 * GIB))
room 0
t_eq "$t_out" "8 $((3 * GIB)) $MAX $((2 * GIB))"

t_case "file cache does not count"
good
SLICE_FILE=$((9 * GIB)) slice_child "$a" $((3 * GIB)) 0 $((2 * GIB))
room 0
t_eq "$t_out" "8 $((3 * GIB)) $MAX $((2 * GIB))"

t_case "shared memory and kernel memory that cannot be reclaimed count"
good
SLICE_SHMEM=$GIB SLICE_KERNEL=$GIB slice_child "$a" $((3 * GIB)) $GIB $((2 * GIB))
room 0
t_eq "$t_out" "7 $((5 * GIB)) $MAX $((2 * GIB))"

t_case "a full slice has no room and says so"
good
slice_child "$a" $((19 * GIB)) 0 $((2 * GIB))
room 0
t_eq "$t_out" "0 $((19 * GIB)) $MAX $((2 * GIB)) room"

t_case "more promised than the max is still room 0"
room 4
t_eq "$t_out" "0 $((27 * GIB)) $MAX $((2 * GIB)) room"

t_case "full pressure above 10 holds"
good
slice_state 12.00
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) pressure"

t_case "full pressure of exactly 10 does not hold, a hundredth more does"
slice_state 10.00
room 0
t_eq "$t_out" "10 0 $MAX $((2 * GIB))"
slice_state 10.01
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) pressure"

t_case "some pressure alone does not hold"
slice_state 0.08 55.00
room 0
t_eq "$t_out" "10 0 $MAX $((2 * GIB))"

t_case "the pressure is printed as the kernel wrote it"
slice_state 3.41 55.00
t_run "$pre"'tehas_slice_pressure; echo "rc=$?"'
t_eq "$t_out" "3.41
rc=0"

t_case "memory.pressure unreadable with children present is unreadable"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
rm "$T_CGROUP/tehas.slice/memory.pressure"
room 0
t_eq "$t_out" "0 $((2 * GIB)) $MAX $((2 * GIB)) unreadable $T_CGROUP/tehas.slice/memory.pressure cannot be read"
t_run "$pre"'tehas_slice_pressure; echo "rc=$? why=[$TEHAS_SLICE_WHY]"'
t_eq "$t_out" "rc=1 why=[$T_CGROUP/tehas.slice/memory.pressure cannot be read]"

t_case "a memory.pressure with no full line is unreadable"
good
echo "some avg10=0.00 avg60=0.00 avg300=0.00 total=0" >"$T_CGROUP/tehas.slice/memory.pressure"
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable $T_CGROUP/tehas.slice/memory.pressure has no full avg60"

t_case "memory.stat unparsable is unreadable"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
slice_child "$b" lots 0 $((2 * GIB))
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable $T_CGROUP/tehas.slice/docker-$b.scope/memory.stat has no number for anon"

t_case "a memory.stat without one of the four numbers is unreadable"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
printf '%s\n' "anon 1" "shmem 0" "kernel 0" >"$T_CGROUP/tehas.slice/docker-$a.scope/memory.stat"
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable $T_CGROUP/tehas.slice/docker-$a.scope/memory.stat has no number for slab_reclaimable"

t_case "a memory.stat with no kernel line counts anon and shmem, not slab"
good
slice_child "$a" $((3 * GIB)) $GIB 0
printf '%s\n' "anon $((3 * GIB))" "shmem $GIB" "slab_reclaimable $GIB" >"$T_CGROUP/tehas.slice/docker-$a.scope/memory.stat"
room 0
t_eq "$t_out" "8 $((4 * GIB)) $MAX $((2 * GIB))"

t_case "a missing memory.low is unreadable, and nothing falls back to memory.current"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
rm "$T_CGROUP/tehas.slice/docker-$a.scope/memory.low"
echo 1 >"$T_CGROUP/tehas.slice/docker-$a.scope/memory.current"
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable $T_CGROUP/tehas.slice/docker-$a.scope/memory.low cannot be read"

t_case "a memory.low that is no number is named"
good
slice_child "$a" $((500 * MIB)) 0 lots
room 0
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable $T_CGROUP/tehas.slice/docker-$a.scope/memory.low is not a number"

t_case "a count that is no number is unreadable"
good
room many
t_eq "$t_out" "0 0 $MAX $((2 * GIB)) unreadable the count of cycles is not a number"

t_case "room without a probe is unreadable, with four numbers all the same"
good
t_run "$pre"'tehas_slice_room 0'
t_eq "$t_out" "0 0 0 $((2 * GIB)) unreadable the slice was not probed"

t_case "room makes no call and writes nothing"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
before=$(find "$T_CGROUP" "$T/home" | sort)
room 1
after=$(find "$T_CGROUP" "$T/home" | sort)
t_eq "$after" "$before"
t_eq "$(wc -l <"$T/calls/systemctl" | tr -d ' ') $(wc -l <"$T/calls/docker" | tr -d ' ')" "1 1"

t_case "the reason after unreadable is read whole by the line the dispatcher uses"
good
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$pre"'tehas_slice_ok; read -r room committed max reserve why note < <(tehas_slice_room 0); echo "[$room] [$why] [$note]"'
t_eq "$t_out" "[0] [unreadable] [$T_CGROUP/tehas.slice/memory.pressure cannot be read]"

t_case "the room is read by the line the dispatcher uses"
good
slice_child "$a" $((500 * MIB)) 0 $((2 * GIB))
t_run "$pre"'tehas_slice_ok; read -r room committed max reserve why < <(tehas_slice_room 2); echo "[$room] [$committed] [$max] [$reserve] [$why]"'
t_eq "$t_out" "[7] [$((6 * GIB))] [$MAX] [$((2 * GIB))] []"

t_done
