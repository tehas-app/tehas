#!/usr/bin/env bash
# Tests for libexec/dispatch: admission by the slice's room and pressure, and
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

# --- Free disk ---------------------------------------------------------------
# The dispatcher starts no cycle while the file system of ~/tehas and the one
# Docker keeps its images on have less free space than TEHAS_CYCLE_MIN_FREE.
# The host of reset has 40 GB free on both, above the default minimum of 10g,
# and Docker's own file system is one of the same size, so a case that wants
# the two apart writes them with df_free_of. The model endpoint is the hold
# that sits above the disk's, so every case here brings its own answer for it:
# a host that gets none from the stub holds on no resources before it ever
# asks df.

# asked <answer>: the endpoint of the host this case builds, and no more.
asked() {
  mesh_curl
  mesh "$1"
}

t_case "a full disk: nothing starts, no label is edited, the dry run says the figure"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7 8
df_free 1
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "api -X PATCH"
t_lacks "$(t_calls gh)" "issue comment"
t_contains "$t_out" "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts"
t_lacks "$t_out" "would run a cycle"

t_case "at the minimum a ready issue starts, and the figure is not said"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7 8
df_free 10
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$t_out" "free on the disk"
t_eq "$(jq -r .why "$dispatch_json")" ""

t_case "one kilobyte under the minimum holds"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7 8
df_free 10
printf '%s\n' "$(($(kb 10) - 1))" >"$fx/free"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 9.9 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts"

t_case "the instance's minimum is the one that is asked of"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7
df_free 30
slots 2 TEHAS_CYCLE_MIN_FREE=40g
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 30.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=40g: no cycle starts"

t_case "the lesser of the two file systems decides"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7 8
df_free_of 40 1
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts"
reset
slots 2
ready 7 8
df_free_of 1 40
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts"

t_case "a disk whose free space cannot be read holds: no df, no docker info, and no number"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7
: >"$fx/df-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_contains "$t_out" "holding: the free disk space could not be read: no cycle starts"
t_eq "$(jq -r .why "$dispatch_json")" "disk"
reset
slots 2
ready 7
# No figure at all: neither file system answers.
df_reset
: >"$fx/df-fails"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the free disk space could not be read: no cycle starts"
reset
slots 2
ready 7
# The file system Docker keeps its images on cannot be read: the figure of
# ~/tehas is no answer for it.
df_free_of 40 1
echo /srv/docker >"$fx/df-fails-of"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the free disk space could not be read: no cycle starts"
reset
slots 2
ready 7
df_reset
echo lots >"$fx/free"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the free disk space could not be read: no cycle starts"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: the free disk space could not be read: no cycle starts"
reset
slots 2
ready 7
# Docker is not there to name the file system of its images, so df is pointed
# at the default, and that one cannot be read: the figure of ~/tehas is no
# answer for it.
df_free 40
echo /var/lib/docker >"$fx/df-fails-of"
: >"$fx/docker-down"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: the free disk space could not be read: no cycle starts"

sha=0123456789abcdef0123456789abcdef01234567
# The one run of the check on the head commit, failed.
checks() {
  jq -nc --arg sha "$sha" '{total_count: 1, check_runs: [{name: "verify",
    head_sha: $sha, status: "completed", conclusion: "failure"}]}' >"$fx/checks.json"
}

t_case "the disk hold comes before the count, the limit and a red branch"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 1
ready 7 8
df_free 1
limit 0 1 '' 'the mesh is busy'
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" "disk"
reset
slots 1 TEHAS_BRANCH_CHECK=verify
ready 7 8
df_free 1
checks completed failure
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" "disk"
t_lacks "$t_out" "is red at"

t_case "the slice and the model endpoint are told of before a full disk"
# The three holds above the disk's keep their place: each of them holds a
# planning turn too, and the disk does not, so a poll that said disk first
# would hold work it must not. The last of the three is a poll whose endpoint
# offers no model of ours: that answer, and only that one, lets the disk be
# read at all, because the endpoint is asked first.
mesh_curl
reset
slots 2
ready 7
df_free 1
no_slice
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" "slice"
t_contains "$t_out" "holding: no slice file on this host"
reset
slots 2
ready 7
df_free 1
rm "$T_CGROUP/tehas.slice/memory.pressure"
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" "unreadable"
reset
slots 2
ready 7
df_free 1
mesh
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" "resources"
t_contains "$t_out" "holding: no resources: the model endpoint does not answer"
t_lacks "$t_out" "free on the disk"

t_case "a full disk does not hold a planning turn"
asked '{"data":[{"id":"test-model"}]}'
to_plan
df_free 1
t_run "$dispatch"
t_contains "$(plan_calls)" "--brief"
t_lacks "$t_out" "would start a planning session"

t_case "a full disk leaves a running cycle alone"
asked '{"data":[{"id":"test-model"}]}'
reset
slots 2
ready 7 8
df_free 1
hold 7
cid 7 a $gib
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 1.0 GiB free on the disk"
release

t_case "a full disk leaves the sweep of kept workspaces and the orphan labels alone"
asked '{"data":[{"id":"test-model"}]}'
reset
no_slice
df_free 1
slice_show $((20 * gib))
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 9 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_eq "$(t_calls fake-start)" ""
t_contains "$t_out" "holding: 1.0 GiB free on the disk"

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

# Which endpoint the subagent provider is on, as the energy sampler reads it:
# the one word in live/dispatch.json. The endpoint itself is never written.
t_case "the poll publishes the subagent provider's endpoint as one word"
reset
T_ENV+=("${keys[@]}")
mesh '{"data":[{"id":"test-model"}]}'
backed '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(jq -r .endpoint "$dispatch_json")" main
t_lacks "$(cat "$dispatch_json")" "mesh.test"

reset
T_ENV+=("${keys[@]}")
mesh '{"data":[{"id":"other"}]}'
backed '{"data":[{"id":"test-model"}]}'
t_run "$dispatch"
t_eq "$(jq -r .endpoint "$dispatch_json")" backup
t_lacks "$(cat "$dispatch_json")" "backup.test"

# The endpoint is settled only when a poll asks of it, so the word is missing
# whenever the poll asks nothing or gets no answer: the sampler then asks both
# endpoints, as it did before the field existed. A provider with no backup is
# still main; a poll that cannot reach either endpoint holds and writes no word.
reset
T_ENV+=("${keys[@]}")
mesh '{"data":[{"id":"test-model"}]}'
rm -f "$fx/backup.json"
t_run "$dispatch"
t_eq "$(jq -r .endpoint "$dispatch_json")" main
t_eq "$(jq -c 'has("endpoint")' "$dispatch_json")" true

reset
T_ENV+=("${keys[@]}")
mesh
backed '{"data":[]}'
t_run "$dispatch"
t_eq "$(jq -r .why "$dispatch_json")" resources
t_eq "$(jq -c 'has("endpoint")' "$dispatch_json")" false

# A role on another provider does not move the word: the subagent's own
# endpoint is the one published.
reset
T_ENV+=("${keys[@]}" TEHAS_TICKET_CONCURRENCY=1 PI_ENSEMBLE_MODEL_DEVELOPER=big-model PI_ENSEMBLE_PROVIDER_DEVELOPER=other)
mesh '{"data":[{"id":"test-model"}]}'
backed '{"data":[{"id":"test-model"}]}'
echo '{"providers":{"test-provider":{"baseUrl":"http://mesh.test/v1"},"other":{"baseUrl":"http://other.test/v1"}}}' >"$T/home/.pi/agent/models.json"
t_run "$dispatch"
t_eq "$(jq -r .endpoint "$dispatch_json")" main
t_lacks "$(cat "$dispatch_json")" "other.test"

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
