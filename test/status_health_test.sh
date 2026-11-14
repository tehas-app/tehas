#!/usr/bin/env bash
# Tests for the checks of tehasbot health that read the host's files: the limit
# tehas limit sets, the disk of the previews, the slice the last poll reported,
# the holds it names, the cycles that stopped writing, and the cost of the
# polls.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/status_lib.sh
. "$(dirname "$0")/status_lib.sh" || exit 1
# The limit tehas limit sets, as health reads it from state/limit.json.
set_limit() { # <json>
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$1" >"$T/home/tehas/state/limit.json"
}

t_case "limit: a limit is reported in the lines and in --json, its reason only in the lines"
reset
set_limit '{"v":1,"cycles":2,"plan":false,"until":"2036-01-01T00:00:00Z","reason":"held for alpha"}'
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "limit     2 cycles at once, planning off, until 2036-01-01T00:00:00Z: held for alpha"
t_eq "$(jq -r .status "$live/health.json")" pass
t_run "$summary --json"
t_eq "$(jq -c .limit <<<"$t_out")" '{"state":"ok","cycles":2,"plan":false,"until":"2036-01-01T00:00:00Z"}'

t_case "limit: no limit prints no line, and --json says none"
reset
t_run "$status && $summary"
t_lacks "$t_out" "limit "
t_run "$summary --json"
t_eq "$(jq -c .limit <<<"$t_out")" '{"state":"none"}'

t_case "limit: expired or unreadable is a warning that says nothing starts"
reset
set_limit '{"v":1,"cycles":2,"plan":true,"until":"2020-01-01T00:00:00Z","reason":""}'
t_run "$status && $summary --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.state, .limit.state]' <<<"$t_out")" '["warn","expired"]'
t_contains "$(jq -r '.reasons[0]' <<<"$t_out")" "Nothing starts: the limit expired at 2020-01-01T00:00:00Z"
reset
set_limit 'not a limit'
t_run "$status && $summary --json"
t_eq "$(jq -c '[.state, .limit]' <<<"$t_out")" '["warn",{"state":"unreadable"}]'
t_contains "$(jq -r '.reasons[0]' <<<"$t_out")" "Nothing starts: the limit cannot be read"

# The fault of a sandbox that cannot push, as the start whose push check was
# refused leaves it (console/cycle/src/prepare.rs): the time, the bot's login
# and git's line. The poll holds every start while it stands, so status and
# health say it rather than leaving the host still with no word of why.
set_push_fault() { # <json>: what to write where the start that failed left it
  mkdir -p "$T/home/tehas/state"
  printf '%s\n' "$1" >"$T/home/tehas/state/push-fault.json"
}

t_case "push: a sandbox that cannot push is a fail, in the lines, the file and --json"
reset
set_push_fault '{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"remote: Permission to acme/widget denied to tehas-bot"}'
t_run "$status && $summary"
t_contains "$t_out" "push      the sandbox cannot push: remote: Permission to acme/widget denied to tehas-bot (tehas-bot)"
t_eq "$(jq -r .status "$live/health.json")" fail
t_contains "$(jq -r .output "$live/health.json")" "The sandbox cannot push: remote: Permission to acme/widget denied to tehas-bot (tehas-bot)"
t_eq "$(jq -r '.checks.pushFault.line' "$live/health.json")" \
  "remote: Permission to acme/widget denied to tehas-bot"
t_run "$status && $summary --json"
t_eq "$(jq -c '[.state, .pushFault.login]' <<<"$t_out")" '["fail","tehas-bot"]'

reset
set_push_fault '{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"denied"}'
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" fail

# The fault off again, and the host passes as it did before.
rm -f "$T/home/tehas/state/push-fault.json"
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_lacks "$t_out" "the sandbox cannot push"

t_case "push: a fault that says nothing still fails, with a line of the host's own"
reset
set_push_fault 'not json'
t_run "$status && $summary"
t_eq "$(jq -r .status "$live/health.json")" fail
t_contains "$t_out" "push      the sandbox cannot push: the sandbox cannot push to origin"

reset
set_push_fault '{"at":"2026-10-17T13:00:00Z","login":"","line":""}'
t_run "$status"
t_contains "$(jq -r .output "$live/health.json")" "The sandbox cannot push: the sandbox cannot push to origin."

t_case "push: a sandbox that cannot push is said before a slice hold or a limit"
reset
set_push_fault '{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"denied"}'
set_limit '{"v":1,"cycles":2,"plan":true,"until":"2020-01-01T00:00:00Z","reason":""}'
t_run "$status"
t_contains "$(jq -r .output "$live/health.json")" "The sandbox cannot push: denied (tehas-bot)"

# The file is written by a process of the host, so the status page does not
# print its text raw: `shown` takes the control and invisible-format characters
# out of every value the page says, as it does for a person's own text. A fault
# whose line carries an escape does not repaint the terminal of whoever runs
# `tehas status`.
t_case "push: control characters of the fault's line cannot reach the terminal"
reset
# The escape and the override are written as the JSON escapes of them: a raw
# control character is not valid in a JSON string, and jq, which the cases of
# this file read the health file with, would refuse the whole file. The words of
# the line stand, the control characters of it do not.
set_push_fault '{"at":"2026-10-17T13:00:00Z","login":"tehas-bot","line":"denied\u001b[31mred\u202e"}'
# `tehasbot health` is the one that reads the file and writes the health page;
# `tehas status` then says what the page holds.
t_run "$health && $summary"
t_contains "$t_out" "push      the sandbox cannot push: denied [31mred (tehas-bot)"
t_eq "$(grep -c 'push      the sandbox cannot push' <<<"$t_out")" 1

t_case "limit: the room under it, from the last poll"
reset
held limit "holding: the limit allows 1 cycle at once and 1 is running" 1 2
jq '.limitRoom = 0' "$live/dispatch.json" >"$live/dispatch.json.new" && mv "$live/dispatch.json.new" "$live/dispatch.json"
t_run "$status && $summary --json"
t_eq "$(jq -c .room <<<"$t_out")" '{"cycles":1,"maxAwaiting":2,"slice":3,"limit":0}'
t_eq "$(jq -c .hold <<<"$t_out")" '{"why":"limit","text":"holding: the limit allows 1 cycle at once and 1 is running"}'

disk() { # low [age in seconds]
  mkdir -p "$live"
  jq -n --argjson low "$1" --arg at "$(date -u -d "@$(($(date +%s) - ${2:-60}))" +%FT%TZ)" \
    '{generatedAt: $at, disk: {freeBytes: 1073741824, minFreeBytes: 5368709120, low: $low}, previews: []}' \
    >"$live/previews.json"
}

t_case "health: a host with no previews has no disk check"
reset
t_run "$status"
t_eq "$(jq -c '.checks.disk' "$live/health.json")" "null"

t_case "health: low disk is a warning, with the figures"
reset
disk true
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "warn"
t_contains "$(jq -r '.output' "$live/health.json")" "low on disk: previews are not being built"
t_eq "$(jq -c '.checks.disk | {freeBytes, minFreeBytes, low}' "$live/health.json")" \
  '{"freeBytes":1073741824,"minFreeBytes":5368709120,"low":true}'
t_eq "$(find "$live" -name health.down | wc -l)" 0

t_case "health: free space that could not be read is a warning that says so"
reset
disk true
jq '.disk.freeBytes = null' "$live/previews.json" >"$live/x" && mv "$live/x" "$live/previews.json"
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "warn"
t_eq "$(jq -r '.output' "$live/health.json")" "The host's free disk space could not be read: previews are not being built."
t_eq "$(jq -c '.checks.disk | {freeBytes, minFreeBytes, low}' "$live/health.json")" \
  '{"freeBytes":null,"minFreeBytes":5368709120,"low":true}'

t_case "health: a figure with low disk keeps the sentence about low disk"
reset
disk true
t_run "$status"
t_eq "$(jq -r '.output' "$live/health.json")" "The host is low on disk: previews are not being built."

t_case "health: no figure and not low is not a warning"
reset
disk false
jq '.disk.freeBytes = null' "$live/previews.json" >"$live/x" && mv "$live/x" "$live/previews.json"
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "pass"

t_case "health: enough disk passes"
reset
disk false
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "pass"

t_case "health: a figure older than half an hour is not believed"
reset
disk true 3600
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "pass"
t_eq "$(jq -c '.checks.disk' "$live/health.json")" "null"

for bad in 'not json' '{"generatedAt":"yesterday","disk":{"low":true}}' '{"disk":{"low":true}}' '[]' '{"generatedAt":7,"disk":"full"}'; do
  t_case "health: a previews.json of '$bad' gives no disk check and no warning"
  reset
  mkdir -p "$live"
  printf '%s\n' "$bad" >"$live/previews.json"
  t_run "$status"
  t_eq "$(jq -r '.status' "$live/health.json")" "pass"
  t_eq "$(jq -c '.checks.disk' "$live/health.json")" "null"
  t_lacks "$(jq -r '.output' "$live/health.json")" "disk"
  t_eq "$(find "$live" -name health.down | wc -l)" 0
done

t_case "health: a failed cycle still wins over low disk"
reset
disk true
cycle false died verify
t_run "$status"
t_eq "$(jq -r '.status' "$live/health.json")" "fail"

# What the dispatcher's last poll wrote about the slice, as libexec/dispatch
# writes live/dispatch.json. 4 GiB of 20 GiB are promised.
poll() { # <why> <hold> [<age in seconds>]
  mkdir -p "$live"
  jq -n --arg why "$1" --arg hold "$2" --arg at "$(date -u -d "@$(($(date +%s) - ${3:-40}))" +%FT%TZ)" \
    '{time: $at, why: $why, hold: $hold, count: 1, room: 8, committed: 4294967296, max: 21474836480, reserve: "2g"}' \
    >"$live/dispatch.json"
}

t_case "health carries checks.slice from live/dispatch.json"
reset
poll "" ""
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c '.checks.slice | del(.time)' "$live/health.json")" \
  '{"why":"","hold":"","count":1,"room":8,"committed":4294967296,"max":21474836480,"reserve":"2g"}'
t_eq "$(jq -r .checks.slice.time "$live/health.json")" "$(jq -r .time "$live/dispatch.json")"

for why in slice unreadable; do
  t_case "health: a hold for '$why' is a warning with the hold as output"
  reset
  poll "$why" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
  t_run "$status; $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" warn
  t_eq "$(jq -r .output "$live/health.json")" "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
  t_eq "$(find "$live" -name health.down | wc -l)" 0
  t_contains "$t_out" "factory   warn: holding: no slice file on this host (sudo ./bootstrap.sh slice)"
  t_contains "$t_out" "slice     holding: no slice file on this host (sudo ./bootstrap.sh slice) (4"
done

t_case "health: a hold on the disk is a warning with the hold as output"
# A full disk, and a disk whose figure cannot be read, both hold every start,
# so /health warns and tehas status carries the line the poll logged. The two
# share the word: neither names a number a person can fix.
for hold in "holding: 1.0 GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts" "holding: the free disk space could not be read: no cycle starts"; do
  t_case "health: the disk hold '$hold' is a warning"
  reset
  poll disk "$hold"
  t_run "$status; $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" warn
  t_eq "$(jq -r .output "$live/health.json")" "$hold"
  t_eq "$(jq -r .checks.slice.why "$live/health.json")" disk
  t_eq "$(find "$live" -name health.down | wc -l)" 0
  t_contains "$t_out" "factory   warn: $hold"
  t_contains "$t_out" "slice     $hold (4"
done

for why in count awaiting pressure room; do
  t_case "health: a hold for '$why' is not a warning, and tehas status still says it"
  reset
  poll "$why" "holding: the reason"
  t_run "$status && $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" pass
  t_contains "$t_out" "factory   pass: Idle, taking work."
  t_contains "$t_out" "slice     holding: the reason (4"
done

t_case "health: no resources is a warning, said by the factory line, and the slice keeps its numbers"
reset
poll resources "holding: no resources: the model endpoint does not answer"
t_run "$status; $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "No resources: the model endpoint does not answer."
t_eq "$(jq -r .checks.slice.why "$live/health.json")" resources
t_eq "$([ -e "$live/health.down" ] && echo down)" ""
t_eq "$(jq -r .noResources "$live/status.json")" true
t_contains "$t_out" "factory   warn: No resources: the model endpoint does not answer."
t_contains "$t_out" "slice     room for 8 cycles"

t_case "status: no resources is not said from a poll older than ten minutes, or from any other hold"
reset
poll resources "holding: no resources: the model endpoint does not answer" 700
t_run "$status"
t_eq "$(jq -r .noResources "$live/status.json")" false
t_eq "$(jq -r .status "$live/health.json")" pass
poll room "holding: tehas.slice has no room for another cycle"
t_run "$status"
t_eq "$(jq -r .noResources "$live/status.json")" false
rm -f "$live/dispatch.json"
t_run "$status"
t_eq "$(jq -r .noResources "$live/status.json")" false

t_case "health: a paused factory is said before no resources"
reset
poll resources "holding: no resources: the model endpoint does not answer"
echo true >"$T/paused"
t_run "$status"
t_eq "$(jq -r .output "$live/health.json")" "The factory is paused."
echo false >"$T/paused"

t_case "health: a paused factory and a failed cycle are said before a slice hold"
reset
poll slice "holding: no slice file on this host (sudo ./bootstrap.sh slice)"
echo true >"$T/paused"
t_run "$status"
t_eq "$(jq -r .output "$live/health.json")" "The factory is paused."
echo false >"$T/paused"
cycle false aborted develop
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" fail

t_case "health: a dispatch.json older than ten minutes gives no slice check and no warning"
reset
poll slice "holding: no slice file on this host (sudo ./bootstrap.sh slice)" 660
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -c .checks.slice "$live/health.json")" null
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "slice     unknown: no poll has reported in 10 min"

for bad in '{"time":' 'not json' '[]' '{"why":"slice","hold":"x"}' '{"time":"yesterday","why":"slice","hold":"x"}' \
  '{"time":7,"why":"slice"}' "{} {}"; do
  t_case "health: a dispatch.json of '$bad' does not break the health write"
  reset
  mkdir -p "$live"
  printf '%s\n' "$bad" >"$live/dispatch.json"
  t_run "$status && $summary"
  t_eq "$t_rc" 0
  t_eq "$(jq -r .status "$live/health.json")" pass
  t_eq "$(jq -c .checks.slice "$live/health.json")" null
  t_contains "$t_out" "slice     unknown"
done

t_case "summary: the slice line says the room, what is promised, the reserve and its age"
reset
# No checkout on this host, so the platform line stays out and the slice line
# is the last of the four.
rm -rf "$T/home/tehas/platform"
poll "" ""
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 4
t_contains "$t_out" "slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, 4"
t_contains "$(printf '%s\n' "$t_out" | tail -n 1)" " s ago)"
poll "" "" 300
jq '.committed = 4831838208' "$live/dispatch.json" >"$live/x" && mv "$live/x" "$live/dispatch.json"
t_run "$status && $summary"
t_contains "$t_out" "slice     room for 8 cycles (4.5 GiB of 20 GiB promised, reserve 2g, 5 min ago)"

t_case "summary: with a refresh lane the slice line says how much of it is free"
reset
rm -rf "$T/home/tehas/platform"
poll "" ""
jq '.refreshRoom = 1' "$live/dispatch.json" >"$live/x" && mv "$live/x" "$live/dispatch.json"
t_run "$status && $summary"
t_contains "$t_out" "slice     room for 8 cycles (4 GiB of 20 GiB promised, reserve 2g, refresh lane 1 free, 4"
t_eq "$(jq -r .checks.slice.refreshRoom "$live/health.json")" 1

t_case "summary: a health file written before the slice was known says unknown"
reset
t_run "$status"
jq 'del(.checks.slice)' "$live/health.json" >"$live/x" && mv "$live/x" "$live/health.json"
t_run "$summary"
t_eq "$t_rc" 0
t_contains "$t_out" "slice     unknown: no poll has reported in 10 min"

t_case "health and summary read files only, and ask systemd only for the failed units"
reset
t_stub systemctl
t_stub docker
poll room "holding: the reason"
t_run "$status && $summary"
t_eq "$(t_calls systemctl)" "--user --failed --plain --no-legend tehas-*"
# Only status asks docker, for the label of the image cycles run (console/ops/src/rukas.rs).
t_eq "$(t_calls docker)" "image inspect --format {{.Id}} {{index .Config.Labels \"tehas.rukas\"}} tehas/pi-rukas:uid$(id -u)"
rm -f "$T"/calls/docker
t_run "$health && $summary"
t_eq "$(t_calls docker)" ""

t_case "status writes the pi-rukas the image runs, and health warns while it is behind"
reset
a=$(printf 'a%.0s' {1..40}) b=$(printf 'b%.0s' {1..40})
t_stub docker "case \"\$1 \$2\" in
  'image inspect') echo 'sha256:1 $a' ;;
  'run --rm') echo '{\"version\":\"0.14.53\"}' ;;
esac"
T_ENV=("PI_RUKAS_FOLLOW=commit:$b")
t_run "$status"
T_ENV=()
t_eq "$t_rc" 0
t_eq "$(jq -c '{follow, target, image, imageVersion, behind}' "$live/rukas.json")" \
  "{\"follow\":\"commit:$b\",\"target\":\"$b\",\"image\":\"$a\",\"imageVersion\":\"v0.14.53\",\"behind\":true}"
t_eq "$(jq -r .checks.rukas.imageVersion "$live/health.json")" v0.14.53
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "The sandbox image runs pi-rukas v0.14.53 and the host follows bbbbbbbb: run tehas update."
t_contains "$(t_calls docker)" "run --rm --init --network none tehas/pi-rukas:uid$(id -u) /opt/pi-rukas/bin/pi-rukas-headless --capabilities"

t_case "an image built from a release newer than the target cached is current, and one older is behind"
reset
b=$(printf 'b%.0s' {1..40}) c=$(printf 'c%.0s' {1..40})
# The target was asked a minute ago, at v0.14.54; v0.14.55 came out since and
# the image was built from it.
mkdir -p "$T/home/tehas/state"
printf '{"at":%s,"follow":"release:0","repo":"https://github.com/up/pi-rukas.git","commit":"%s","what":"release v0.14.54"}\n' \
  "$(($(date +%s) - 60))" "$b" >"$T/home/tehas/state/rukas-target"
t_stub git "case \"\$1\" in
  ls-remote) printf '%s\trefs/tags/v0.14.54\n%s\trefs/tags/v0.14.55\n' $b $c ;;
  *) command -p git \"\$@\" ;;
esac"
t_stub docker "case \"\$1 \$2\" in
  'image inspect') echo 'sha256:2 $c' ;;
  'run --rm') echo '{\"version\":\"0.14.55\"}' ;;
esac"
T_ENV=("PI_RUKAS_FOLLOW=release:0" "PI_RUKAS_REPO=https://github.com/up/pi-rukas.git")
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -c '{target, targetVersion, imageVersion, behind}' "$live/rukas.json")" \
  "{\"target\":\"$c\",\"targetVersion\":\"v0.14.55\",\"imageVersion\":\"v0.14.55\",\"behind\":false}"
t_eq "$(jq -r .checks.rukas.behind "$live/health.json")" false
# An image a release older than the target is still behind it.
t_stub docker "case \"\$1 \$2\" in
  'image inspect') echo 'sha256:1 $b' ;;
  'run --rm') echo '{\"version\":\"0.14.54\"}' ;;
esac"
t_run "$status"
T_ENV=()
rm -f "$T/bin/git"
t_stub docker 'exit 1'
t_eq "$(jq -c '{targetVersion, imageVersion, behind}' "$live/rukas.json")" \
  '{"targetVersion":"v0.14.55","imageVersion":"v0.14.54","behind":true}'
t_eq "$(jq -r .checks.rukas.behind "$live/health.json")" true

t_case "a lost cycle ends two minutes after its last heartbeat"
reset
one 9 true running develop 900 300
one 12 false merged review 600 200
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."
t_eq "$(jq -c '[.last.issue, .last.status]' "$live/running.json")" '[9,"lost"]'
t_eq "$(find "$live/cycles" -name '*.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "9.json "

t_case "health warns when the state store has no backup, or only one more than two days old"
reset
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -r '.checks.backup.ageSeconds < 60' "$live/health.json")" true
rm -rf "$T/home/tehas/state/backup"
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "There is no backup of the state store: tehasbot clean makes one a day."
t_eq "$(jq -c .checks.backup "$live/health.json")" null
TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null
touch -d '72 hours ago' "$T"/home/tehas/state/backup/tehas-*.db
t_run "$health"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "The newest backup of the state store is 72 h old."

# The cost of the polls that started nothing, read from the state store beside
# the backup. A poll of the factory the plan of #730 describes costs about 3 s
# of CPU and 20 s of wall, so the default budget of cpu=5,wall=30,writes=0
# leaves it alone and warns at one that costs more than that.
poll_record() { # <cpu_seconds> <wall_seconds> [WRITES] [STARTS]: one poll of
# the hour on record, whose wall time is the second it names, asked of the
# store's own clock. WRITES left out is no write log at all, which leaves the
# figure off the answer; 0 is a log of none, which the store says as none and
# no budget of writes can be over.
  local cpu=$1 wall=$2 writes=${3-} starts=${4:-0} ms n now
  now=$(date +%s)
  rm -f "$T/poll.writes"
  if [ -n "$writes" ]; then
    : >"$T/poll.writes"
    ms=$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)
    for ((n = 0; n < writes; n++)); do
      printf '%s\tissue edit\t\n' "$ms" >>"$T/poll.writes"
    done
  fi
  # The wall time is the difference of the two times the store reads from its
  # own clock, so both name the same second and the figure is the one asked for.
  t_at "$now" poll-record --started "$((now - wall))" --cpu-ms "$((cpu * 1000))" --gh 0 \
    --writes-log "$([ -n "$writes" ] && printf %s "$T/poll.writes" || printf %s "$T/no-such.log")" \
    --starts "$starts" >/dev/null || t_bad "no poll recorded"
}

t_case "an idle poll within the budget is no warning, and the check is on record"
reset
poll_record 3 20
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -rc .checks.poll_cost "$live/health.json")" '{"over":[],"budget":"cpu=5,wall=30,writes=0"}'

t_case "an idle poll over its CPU budget warns and names only that figure"
reset
poll_record 6 20
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '["cpu"]'
t_eq "$(jq -r .output "$live/health.json")" \
  "An idle poll is over its budget on cpu (TEHAS_POLL_BUDGET=cpu=5,wall=30,writes=0): of the last hour's 1 idle poll, cpu had a median of 6 s against 5 s and 1 over it."

t_case "an idle poll over all three budgets names them in the order of the spec"
reset
poll_record 6 40 2
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '["cpu","wall","writes"]'

t_case "a poll of a write log that holds none passes the writes of the default budget"
reset
poll_record 3 20 0
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '[]'

t_case "one write is over the writes the default budget allows, which are none"
reset
poll_record 3 20 1
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '["writes"]'

# The budget the instance set is the one the store is asked to measure against,
# and the health file carries it so a reader sees the figures beside their
# ceiling. The answer of the verb, for those arguments only, gives an over
# figure so the case does not lean on the store's arithmetic, which the
# store's own tests cover.
t_case "the budget of the instance is what the store is asked for, and the health file says it"
reset
t_store_answers "poll-idle --since 3600 --budget cpu=10,wall=10" 0 '{"api":1,"ok":true,"verb":"poll-idle","polls":1,"cpu_ms":6000,"wall_ms":20000,"writes":0,"over":["cpu"]}'
T_ENV=(TEHAS_INSTANCE_DIR="$T/budget-instance")
mkdir -p "$T/budget-instance"
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_POLL_BUDGET=cpu=10,wall=10" >"$T/budget-instance/instance.env"
poll_record 3 20
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -rc .checks.poll_cost "$live/health.json")" '{"over":["cpu"],"budget":"cpu=10,wall=10"}'
t_eq "$(t_store_answered)" 1
T_ENV=()

t_case "a poll that started a cycle is not an idle poll and costs no warning"
reset
poll_record 60 60 3 1
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -rc '.checks.poll_cost.over' "$live/health.json")" '[]'
t_eq "$(jq -rc .checks.poll_cost.budget "$live/health.json")" cpu=5,wall=30,writes=0

t_case "the store's refusal is no warning, and the health file says there was no check"
reset
poll_record 6 60
t_store_answers poll-idle 1 'store: busy: the store is held'
t_run "$status"
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c .checks.poll_cost "$live/health.json")" null

t_done
