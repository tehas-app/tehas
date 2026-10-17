#!/usr/bin/env bash
# Tests for libexec/dispatch: admission by the slice's room and pressure, and
# the model endpoint and its backup.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh"

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
t_eq "$(cat "$park_id")" 4001
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
t_eq "$(cat "$park_id")" 4001

t_case "no slice: a waiting planning answer is left as it is, no label is edited, no comment"
planning
comments 7 "$(plan_body ' ' x ' ')" alice
no_slice
before=$(cat "$record")
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls gh)" "api -X"
t_lacks "$(t_calls gh)" "/comments -f"
t_eq "$(cat "$record")" "$before"
t_eq "$([ -e "$record.answer" ] && echo written)" ""
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
t_eq "$([ -e "$record" ] && echo kept)" ""

t_case "no slice: a plan label starts no session and keeps its label"
to_plan
no_slice
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(plan_calls)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$([ -e "$record" ] || [ -e "$record.brief" ] && echo written)" ""
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

t_case "a real poll logs a hold once, then released once, and writes live/dispatch.json"
reset
echo '[]' >"$fx/ready.json"
other $((19 * gib + gib / 2))
t_run "$dispatch --dry-run"
t_eq "$([ -e "$dispatch_json" ] && echo written)" ""
t_run "$dispatch"
t_eq "$(grep -c 'holding: tehas.slice has no room' <<<"$t_out")" 1
# The status is written once more when a hold begins or ends, for tehas status.
t_eq "$(t_calls fake-status | wc -l)" 2
t_eq "$(jq -c '[.why, .count, .room, .committed, .max, .reserve]' "$dispatch_json")" \
  "[\"room\",0,0,$((19 * gib + gib / 2)),$((20 * gib)),\"2g\"]"
t_eq "$(jq -r .hold "$dispatch_json")" "holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)"
t_eq "$(jq -r '.time | fromdateiso8601 | (now - .) < 60' "$dispatch_json")" true
t_eq "$(find "$T/home/tehas/live" -name 'dispatch.json.*' | wc -l)" 0
rm -f "$T/calls/fake-status"
t_run "$dispatch"
t_lacks "$t_out" "holding:"
t_eq "$(t_calls fake-status | wc -l)" 1
other "$gib"
t_run "$dispatch"
t_eq "$(grep -c 'released: cycles may start again' <<<"$t_out")" 1
t_eq "$(t_calls fake-status | wc -l)" 3
t_lacks "$t_out" "holding:"
t_eq "$(jq -c '[.why, .hold, .room, .committed]' "$dispatch_json")" "[\"\",\"\",3,$((2 * gib))]"
t_run "$dispatch"
t_lacks "$t_out" "released"

mesh_curl

t_case "an endpoint that offers the model holds nothing back"
reset
mesh '{"data":[{"id":"other"},{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls curl)" "http://mesh.test/v1/models"
t_eq "$(jq -r .why "$dispatch_json")" ""

while IFS='|' read -r answer text; do
  t_case "no resources: $text"
  reset
  if [ "$answer" = none ]; then mesh; else mesh "$answer"; fi
  t_run "$dispatch"
  t_eq "$t_rc" 0
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_eq "$(grep -c "holding: no resources: $text" <<<"$t_out")" 1
  t_eq "$(jq -c '[.why, .hold]' "$dispatch_json")" "[\"resources\",\"holding: no resources: $text\"]"
  t_lacks "$(cat "$dispatch_json")" "mesh.test"
done <<'CASES'
none|the model endpoint does not answer
not json|the model endpoint offers no models
{"data":[]}|the model endpoint offers no models
{"data":[{"id":"other"}]}|the model endpoint does not offer test-model
CASES

t_case "a role's own model the endpoint does not offer holds the start, and the endpoint is asked once"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
mesh '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(grep -c "holding: no resources: the model endpoint does not offer small-model" <<<"$t_out")" 1
t_eq "$(t_calls curl | grep -c '/models$')" 1
mesh '{"data":[{"id":"test-model"},{"id":"small-model"}]}'
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a role on a provider of its own is asked of that provider's endpoint"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_PROVIDER_DEVELOPER=other
mesh '{"data":[{"id":"test-model"}]}'
echo '{"providers":{"test-provider":{"baseUrl":"http://mesh.test/v1"},"other":{"baseUrl":"http://other.test/v1"}}}' >"$T/home/.pi/agent/models.json"
t_run "$dispatch"
t_contains "$(t_calls curl)" "http://other.test/v1/models"
t_eq "$(grep -c "holding: no resources: the model endpoint does not offer big-model" <<<"$t_out")" 1
t_lacks "$(cat "$dispatch_json")" "other.test"

# A main endpoint and a backup, each with its own token. The stand-in for curl
# answers main.test from mesh.json and backup.test from backup.json.
backed() { # [<the backup's answer>]
  mkdir -p "$T/home/.pi/agent"
  jq -n '{providers: {"test-provider": {baseUrl: "http://mesh.test/v1", apiKey: "$TEHAS_MODEL_KEY_MAIN",
    backupBaseUrl: "http://backup.test/v1", backupApiKey: "$TEHAS_MODEL_KEY_BACKUP"}}}' >"$T/home/.pi/agent/models.json"
  rm -f "$fx/backup.json"
  [ "$#" = 0 ] || echo "$1" >"$fx/backup.json"
}
t_stub curl 'for a; do case "$a" in /dev/fd/*) cat "$a" >>"$T/calls/curl-config" ;; esac; done
case "$*" in
  *"/chat/completions") cat >"$T/calls/curl-body"; [ -e "$T/fx/reads-images" ] ;;
  *backup.test*) cat "$T/fx/backup.json" 2>/dev/null ;;
  *) cat "$T/fx/mesh.json" 2>/dev/null ;;
esac'
keys=(TEHAS_MODEL_KEY_MAIN=main-secret TEHAS_MODEL_KEY_BACKUP=backup-secret)

t_case "a main endpoint that offers the model is used, with its token, and the backup is not asked"
reset
T_ENV+=("${keys[@]}")
mesh '{"data":[{"id":"test-model"}]}'
backed '{"data":[{"id":"test-model"}]}'
rm -f "$T/calls/curl-config"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_lacks "$(t_calls curl)" "backup.test"
t_eq "$(cat "$T/calls/curl-config")" 'header = "Authorization: Bearer main-secret"'
t_lacks "$(t_calls curl)" "secret"
t_lacks "$t_out" "backup"

t_case "a main endpoint that fails starts the cycle on the backup, and says so"
for main in none '{"data":[{"id":"other"}]}'; do
  reset
  T_ENV+=("${keys[@]}")
  if [ "$main" = none ]; then mesh; else mesh "$main"; fi
  backed '{"data":[{"id":"test-model"}]}'
  rm -f "$T/calls/curl-config"
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" "7"
  t_contains "$(t_calls curl)" "http://backup.test/v1/models"
  t_contains "$(cat "$T/calls/curl-config")" "Bearer backup-secret"
  t_contains "$t_out" "model endpoint: provider test-provider is on its backup"
  t_eq "$(jq -r .why "$dispatch_json")" ""
done

t_case "with both endpoints failing the start is held, and both reasons are given"
reset
T_ENV+=("${keys[@]}")
mesh
backed '{"data":[]}'
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(jq -r .hold "$dispatch_json")" "holding: no resources: the model endpoint does not answer, and its backup offers no models"
t_lacks "$(cat "$dispatch_json")" "test/v1"

t_case "a token models.json names and the host has not set stops the dispatcher: exit 78"
reset
T_ENV+=(TEHAS_MODEL_KEY_MAIN=main-secret)
backed '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json names TEHAS_MODEL_KEY_BACKUP and it is not set"
t_eq "$(t_calls curl)$(t_calls fake-start)" ""
mesh_curl

t_done
