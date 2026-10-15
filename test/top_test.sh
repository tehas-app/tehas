#!/usr/bin/env bash
# Tests for tehas top: the snapshot in lib/top.sh, the frame drawn from it, and
# the command in libexec/top that wires the two. Everything runs against files
# written here under $TEHAS_HOME/live/, with systemctl stubbed. Nothing here
# calls GitHub, a model or Docker.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

top=$T_ROOT/libexec/top
live=$T/home/tehas/live

iso() { date -u -d "@$(($(date +%s) - ${1:-30}))" +%FT%TZ; }

# A systemctl that answers: two units, one timer that fires later.
unit_stub() {
  t_stub systemctl '
case " $* " in
  *" list-units "*)
    printf "%s\n" "tehas-dispatch.timer loaded active waiting Dispatch the queue" \
                  "tehas-cycle@12.service loaded active running Cycle for 12" ;;
  *" list-unit-files "*)
    printf "%s\n" "tehas-dispatch.timer enabled" "tehas-energy.service enabled" ;;
  *" show "*)
    printf "Id=tehas-cycle@12.service\nLoadState=loaded\nActiveState=active\nSubState=running\nUnitFileState=\nNextElapseUSecRealtime=n/a\n\nId=tehas-dispatch.timer\nLoadState=loaded\nActiveState=active\nSubState=waiting\nUnitFileState=enabled\nNextElapseUSecRealtime=Mon 2026-01-01 10:00:00 UTC n/a\n\nId=tehas-energy.service\nLoadState=loaded\nActiveState=inactive\nSubState=dead\nUnitFileState=enabled\nNextElapseUSecRealtime=n/a\n" ;;
  *) exit 0 ;;
esac'
}

reset() {
  rm -rf "$T"/calls/* "$live"
  mkdir -p "$live"
  T_ENV=()
  unit_stub
}

# The files a working factory leaves behind. Times are seconds before now, so
# the words about age stay in one bucket while a case runs.
files() { # [title]
  local title=${1:-Two cycles side by side}
  jq -n --arg title "$title" --arg at "$(iso 210)" '{
    generatedAt: $at, repo: "acme/widget", paused: false,
    running: [{number: 12, title: $title, url: "u", updatedAt: "x", closedAt: null}],
    queued: [{number: 42, title: "tehas top", url: "u", updatedAt: "x", closedAt: null}],
    awaiting: [{number: 7, title: "A preview for every pull request", url: "u", updatedAt: "x", closedAt: null}],
    stopped: [{number: 9, title: "A stopped cycle", url: "u", updatedAt: "x", closedAt: null}],
    completed: []}' >"$live/status.json"
  jq -n --argjson now "$(date +%s)" '{cycles: [{issue: 12, running: true, status: "running",
      step: "develop", console: "console-12.txt", startedAt: ($now - 630), updatedAt: ($now - 10)}],
     last: null}' >"$live/running.json"
  jq -n '{status: "pass", time: $t, output: "Working on #12.", checks: {queue: {time: $q, paused: false,
      inLine: 1, building: 1, review: 1, stopped: 1}, cycle: null, cycles: []}}' \
    --arg t "$(iso 210)" --arg q "$(iso 210)" >"$live/health.json"
  printf '%s\n' '[step develop] @developer ran the gate' 'ok    the shell tests' \
    >"$live/console-12.txt"
}

energy() { # age in seconds
  jq -n --arg at "$(iso "${1:-20}")" '{generatedAt: $at, totals: {watts: 312.5, hosts: 4},
    models: [{name: "m", slots: 8, busy: 4}]}' >"$live/energy.json"
}

# Bytes that are not printing characters, tab and newline excepted.
control_bytes() { printf '%s' "$1" | LC_ALL=C tr -cd '\000-\010\013-\037\177' | wc -c; }

t_case "--json gives one object: generatedAt and one key per section"
reset
files
energy
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[(.generatedAt | type), (keys | length)] | join(" ")' <<<"$t_out")" "string 7"
t_eq "$(jq -r '[.factory, .queue, .cycle, .units, .energy, .console | type] | join(",")' <<<"$t_out")" \
  "object,object,object,array,object,object"
t_eq "$(jq -r '.factory.status, .queue.counts.inLine, .cycle.cycles[0].issue, .console.name' <<<"$t_out")" \
  "pass
1
12
console-12.txt"

t_case "the snapshot reads the live files and asks systemd: nothing else"
reset
files
t_run "$top --json"
t_eq "$t_rc" 0
t_contains "$(t_calls systemctl)" "list-units --all --full --plain --no-legend tehas-*"
t_contains "$(t_calls systemctl)" "show -p Id,LoadState,ActiveState,SubState,UnitFileState,NextElapseUSecRealtime"

t_case "a source that is not there is null, and the others are still there"
reset
files
rm -f "$live/energy.json" "$live/console-12.txt" "$live/running.json"
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.factory, .queue, .cycle, .units, .energy, .console | . == null]' <<<"$t_out")" \
  "[false,false,true,false,true,true]"

t_case "every source missing is null for its key, and --json still exits 0"
reset
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.factory, .queue, .cycle, .energy, .console] | map(. == null)' <<<"$t_out")" \
  "[true,true,true,true,true]"

t_case "a snapshot fed to the renderer gives the same frame as --once"
reset
files
t_run "$top --json"
t_run '. "$T_ROOT/lib/top.sh"; snapshot=$("$T_ROOT/libexec/top" --json); tehas_top_frame "$snapshot" 80 24 >"$T/by-snapshot"; "$T_ROOT/libexec/top" --once >"$T/by-files"'
t_eq "$t_rc" 0
# The first line names the moment the snapshot was taken, which is later for
# one of the two runs. The frame below it is the same.
t_eq "$(tail -n +2 "$T/by-snapshot")" "$(tail -n +2 "$T/by-files")"

t_case "a frame with a running cycle has all six sections"
reset
files
energy
t_run "$top --once"
t_eq "$t_rc" 0
for line in "tehas top" "factory   pass: Working on #12." "queue     1 in line, 1 building, 1 in review, 1 stopped" \
  "  12     running    Two cycles side by side" "cycle     #12 running, step develop, started 10 min ago" \
  "units     3 units, 2 active" "  tehas-dispatch.timer        enabled  active  next 10:00:00" \
  "energy    312.5 W now, 4 of 8 slots busy" "console   #12  console-12.txt  live" \
  "[step develop] @developer ran the gate"; do
  t_contains "$t_out" "$line"
done

t_case "a cycle that ended is shown as the last one, marked ended"
reset
files
jq -n --argjson now "$(date +%s)" '{cycles: [], last: {issue: 12, running: false, status: "handoff",
  step: "review", console: "console-12.txt", startedAt: ($now - 900), updatedAt: ($now - 120)}}' \
  >"$live/running.json"
t_run "$top --once"
t_contains "$t_out" "cycle     #12 handoff at step review, 2 min ago"
t_contains "$t_out" "console   #12  console-12.txt  ended"

t_case "a missing status.json is one line, and the rest of the frame is there"
reset
files
rm -f "$live/status.json"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "queue     no snapshot yet"
t_lacks "$t_out" "Two cycles side by side"
t_contains "$t_out" "factory   pass: Working on #12."
t_contains "$t_out" "cycle     #12 running"

t_case "a missing running.json is one line, and the rest of the frame is there"
reset
files
rm -f "$live/running.json"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "cycle     no cycle written yet"
t_contains "$t_out" "console   no console file"
t_contains "$t_out" "queue     1 in line"

t_case "a missing energy.json is one line, and the rest of the frame is there"
reset
files
energy
rm -f "$live/energy.json"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "energy    no sample"
t_contains "$t_out" "cycle     #12 running"

t_case "a missing console file is one line, and the rest of the frame is there"
reset
files
rm -f "$live/console-12.txt"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "console   no console file"
t_contains "$t_out" "cycle     #12 running"

t_case "an energy file older than ten minutes is not believed"
reset
files
energy 700
t_run "$top --json"
t_eq "$(jq -c '.energy' <<<"$t_out")" "null"
t_run "$top --once"
t_contains "$t_out" "energy    no sample"

t_case "a health file older than ten minutes reads stale, and is still shown"
reset
files
jq --arg t "$(iso 700)" '.time = $t' "$live/health.json" >"$T/h" && mv "$T/h" "$live/health.json"
T_ENV=(COLUMNS=200 LINES=24)
t_run "$top --json"
t_eq "$(jq -r '.factory.stale' <<<"$t_out")" "true"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   stale: the dispatcher last reported 11 min ago"
t_contains "$t_out" "cycle     #12 running"

t_case "a width of 40 and of 200 columns gives no line longer than the width"
reset
files
energy
T_ENV=(COLUMNS=40 LINES=16)
t_run "$top --once"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | awk 'length > 40' | wc -l)" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 16
T_ENV=(COLUMNS=200 LINES=16)
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | awk 'length > 200' | wc -l)" 0

t_case "a short terminal drops console lines, not the sections"
reset
files
energy
# Named lines: a bare number could match the snapshot's clock.
seq -f 'console line %g' 1 30 >"$live/console-12.txt"
T_ENV=(COLUMNS=80 LINES=12)
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 12
for line in "factory   pass" "queue     1 in line" "cycle     #12 running" "units     3 units" \
  "energy    312.5 W" "console   #12"; do
  t_contains "$t_out" "$line"
done
t_lacks "$t_out" "console line 30"

t_case "an escape sequence in a title or a console line does not reach the output"
reset
files "$(printf '\033[31mRed title')"
printf '%s\n' "$(printf '\033]0;evil\007 console line')" >"$live/console-12.txt"
t_run "$top --once"
t_eq "$t_rc" 0
t_eq "$(control_bytes "$t_out")" 0
t_contains "$t_out" "Red title"
t_contains "$t_out" "evil console line"

t_case "standard output that is not a terminal gets no escape codes"
reset
files
energy
t_run "$top --once"
t_eq "$(control_bytes "$t_out")" 0
t_run "$top"
t_eq "$t_rc" 0
t_eq "$(control_bytes "$t_out")" 0
t_contains "$t_out" "tehas top   snapshot"

t_case "systemd that gives no answer is a null section and one line"
reset
unit_stub
t_stub systemctl 'echo "Failed to connect to bus: No such file or directory" >&2; exit 1'
files
t_run "$top --json"
t_eq "$(jq -c '.units' <<<"$t_out")" "null"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "units     systemctl --user gave no answer"
t_contains "$t_out" "cycle     #12 running"

t_case "systemd with no tehas units is an empty section and one line"
reset
t_stub systemctl 'exit 0'
files
t_run "$top --json"
t_eq "$(jq -c '.units' <<<"$t_out")" "[]"
t_run "$top --once"
t_contains "$t_out" "units     no tehas units"

t_case "--every takes a whole number from 1 to 64"
reset
files
T_ENV=(LINES=10 COLUMNS=80)
t_run "$top --once --every 5"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas top   snapshot"
for bad in 0 65 100 -1 abc "" "5x"; do
  t_run "$top --every $bad"
  t_eq "$t_rc" 64
done
t_run "$top --every"
t_eq "$t_rc" 64
t_run "$top --every"
t_contains "$t_out" "--every wants a whole number from 1 to 64"

t_case "an unknown argument is exit 64 with one line"
reset
files
t_run "$top --watch"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas top: unknown argument: --watch"
t_eq "$(t_calls systemctl)" ""

t_case "no live directory at all is exit 1"
reset
rm -rf "$T/home/tehas"
t_run "$top --once"
t_eq "$t_rc" 1
t_contains "$t_out" "no live directory"

t_case "a run changes nothing under TEHAS_HOME"
reset
files
energy
before=$(cd "$live" && cksum ./* | LC_ALL=C sort)
T_ENV=(COLUMNS=80 LINES=20)
t_run "$top --once && $top --json >/dev/null"
t_eq "$t_rc" 0
t_eq "$(cd "$live" && cksum ./* | LC_ALL=C sort)" "$before"
t_eq "$(cd "$live" && find . -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "console-12.txt energy.json health.json running.json status.json "

t_case "the frame comes from the snapshot alone: a snapshot with nothing in it"
reset
printf '%s' '{"generatedAt":"2026-01-01T00:00:00Z"}' >"$T/snap"
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_frame "$(cat "$T/snap")" 80 24'
t_eq "$t_rc" 0
for line in "factory   no health file yet" "cycle     no cycle written yet" \
  "units     systemctl --user gave no answer" "energy    no sample" "console   no console file"; do
  t_contains "$t_out" "$line"
done

t_done
