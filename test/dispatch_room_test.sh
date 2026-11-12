#!/usr/bin/env bash
# Tests for tehasbot dispatch: admission by the slice's room and pressure, and
# the model endpoint and its backup.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# Admission by the slice. Every sandbox container runs in tehas.slice; a
# cycle starts only while the slice has one more reserve of room. reset gives
# a slice of 20 GiB and the default reserve of 2g.
t_case "no room: nothing starts, no label is edited, the tick stays, exit 0, the dry run names the room"
parked
comments 7 "$(park_body x ' ')" alice
ready 8
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(park_q)" 4001
t_contains "$t_out" "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)"

t_case "a cold start is limited by the reserve: with room for two, two of four start"
reset
slots 4
ready 7 8 9 10
other $((15 * gib + gib / 2))
t_run "$dispatch --dry-run"
t_contains "$t_out" "0 cycles running, room for 2 more"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$(t_calls gh)" "issue edit 9 "

t_case "running cycles with no container yet count one reserve each"
reset
slots 4
ready 7 8
other $((13 * gib))
hold 9
hold 10
t_run "$dispatch --dry-run"
t_contains "$t_out" "2 cycles running, room for 1 more"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "a running cycle's container counts at its floor or its use, whichever is more"
reset
slots 6
ready 7 8 9
other $((10 * gib))
hold 9
hold 10
# One young cycle under its floor and one grown past it: 2 and 5 GiB.
cid 9 a $((gib / 2))
cid 10 b $((5 * gib))
t_run "$dispatch --dry-run"
t_contains "$t_out" "2 cycles running, room for 1 more"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "another container's memory lowers the room while cycles are young"
reset
slots 6
ready 7 8 9 10
slice_show $((12 * gib))
hold 12
cid 12 a $((gib / 10))
# A container at 7 GiB is not hidden inside the cycle's floor: 2 and 7 GiB
# are promised, and one more reserve fits in the 3 GiB that are left.
other $((7 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 "
release

t_case "reclaimable slab and file cache do not lower the room"
reset
slots 6
ready 7 8 9 10 11 12
slice_show $((12 * gib))
SLICE_FILE=$((8 * gib)) slice_child "$(slice_id d)" "$gib" $((6 * gib)) $((2 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 "

t_case "full pressure holds over half the slice promised, some pressure does not"
reset
slice_state 12.00
other $((11 * gib))
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "holding: tehas.slice is under memory pressure (full avg60 12.00)"
reset
slice_state 0.00 55.00
other $((11 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "holding:"
# One container thrashing at its own ceiling in a slice mostly empty.
reset
slice_state 12.00
other $((5 * gib))
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "holding:"

t_case "an unreadable probe holds and names the file"
reset
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "holding: the slice's use could not be read: $T_CGROUP/tehas.slice/memory.pressure cannot be read"
t_eq "$(jq -r .hold "$dispatch_json")" "holding: the slice's use could not be read: $T_CGROUP/tehas.slice/memory.pressure cannot be read"
reset
other "$gib"
echo "anon many" >"$T_CGROUP/tehas.slice/docker-$(slice_id d).scope/memory.stat"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: the slice's use could not be read: $T_CGROUP/tehas.slice/docker-$(slice_id d).scope/memory.stat has no number for anon"
t_lacks "$t_out" "would run a cycle"
# A planning turn is held too: its container would start in a slice nobody
# can see into, after its label and comment were spent.
to_plan
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "holding: the slice's use could not be read"
t_eq "$(jq -r .why "$dispatch_json")" unreadable
planning
comments 7 "$(plan_body ' ' x ' ')" alice
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "/comments -f"

t_case "an unreadable probe is named before a full count"
# The hold on planning has to be the hold that is logged, so it comes before
# the count, which holds cycles only.
to_plan
hold 9
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$t_out" "holding: the slice's use could not be read"
t_lacks "$t_out" "cycles are running"
t_eq "$(jq -r .why "$dispatch_json")" unreadable
release

t_case "a missing cgroup directory under running cycles holds while the slice is active"
reset
slots 2
rm -rf "$T_CGROUP/tehas.slice"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the slice's use could not be read"
release
# A cycle between containers in a slice that is not active is one reserve.
reset
slots 2
slice_show $((20 * gib)) inactive
rm -rf "$T_CGROUP/tehas.slice"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$t_out" "holding:"
t_eq "$(jq -r '"\(.room) \(.committed)"' "$dispatch_json")" "3 $((2 * gib))"
release
# With nothing running and the slice not active, no directory is an empty slice.
reset
slice_show $((20 * gib)) inactive
rm -rf "$T_CGROUP/tehas.slice"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "no slice: the hold is logged in a real poll and nothing is labelled"
reset
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "awaiting-merge"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
t_lacks "$t_out" "would run a cycle"

t_case "a cgroupfs driver holds the same way"
reset
echo "cgroupfs 2" >"$fx/docker.info"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: Docker runs containers with the cgroupfs cgroup driver on cgroup v2, tehas needs systemd on v2"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "no slice: a ticked retry is left as it is"
parked
comments 7 "$(park_body x ' ')" alice
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_eq "$(park_q)" 4001

t_case "no slice: a waiting planning answer is left as it is, no label is edited, no comment"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
no_slice
before=$(rec)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "/comments -f"
t_eq "$(rec)" "$before"
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-answer.*' 2>/dev/null | wc -l)" 0
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: no slice file on this host"
t_lacks "$t_out" "would run a planning turn"
# The same answer runs its turn once the slice is there.
slice_show $((20 * gib))
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"

t_case "no slice: the way out of a planning session still ends it"
planning
comments 7 "$(plan_body ' ' ' ' x)" alice
no_slice
t_run "$dispatch"
t_eq "$(plan_calls)" ""
t_contains "$(t_calls gh)" "Planning session ended by alice."
t_eq "$(rec_open)" none

t_case "no slice: a plan label starts no session and keeps its label"
to_plan
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(rec)" null
t_eq "$(find "$T/home/tehas/tmp" -name 'plan-7.brief.*' 2>/dev/null | wc -l)" 0
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: no slice file on this host"
t_lacks "$t_out" "would start a planning session"

t_case "no room still does not hold planning"
to_plan
other $((19 * gib + gib / 2))
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
slice_state 12.00
t_run "$dispatch"
t_contains "$(plan_calls)" "--session abc-123"

t_case "the orphan sweep still runs"
reset
no_slice
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-start)" ""

t_done
