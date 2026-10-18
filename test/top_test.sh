#!/usr/bin/env bash
# Tests for tehas top: the snapshot in lib/top.sh, the frame drawn from it, the
# command in libexec/top that wires the two, and the publisher in
# libexec/top-publish that writes the snapshot for the dashboard. Everything
# runs against files written here under $TEHAS_HOME/live/ and a tree of host
# numbers written here, with systemctl stubbed. Nothing here calls GitHub, a
# model or Docker, and no case reads the machine that runs it.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

top=$T_ROOT/libexec/top
publish=$T_ROOT/libexec/top-publish
live=$T/home/tehas/live
proc1=$T/proc1
proc2=$T/proc2

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

# The host's own numbers, as files a reading can be taken from: two CPUs, load
# averages, memory and four processes. One of them has a command line with a
# token in it, which no reading is allowed to open.
#
# proc1 is the same at both of its readings, so its CPU figures measure nothing.
# proc_files "$proc2" 1005 ... is proc1 five seconds on: tehas-cycle used 2.5 s
# of CPU in those five, pi-rukas 0.1 s, systemd 0.05 s, and the aggregate cpu
# line moved by 450 ticks of which 300 were idle. Read together they give 50%,
# 2% and 1%, and 33.3% over the host - what changed between two readings, not
# the average since a process started, which for these three would be 1.5%, 0%
# and 7.3%.
proc_files() { # <dir> <clock seconds> "<cpu user nice system idle iowait irq softirq steal>" <row>..
  local dir=$1 clock=$2 cpu=$3 row pid name ticks rss
  shift 3
  mkdir -p "$dir"
  printf '%s.00 0.00\n' "$clock" >"$dir/uptime"
  printf 'cpu  %s\ncpu0 1 0 0 0 0 0 0 0 0 0\ncpu1 1 0 0 0 0 0 0 0 0 0\nintr 1\n' "$cpu" >"$dir/stat"
  printf '0.10 0.20 0.30 1/200 300\n' >"$dir/loadavg"
  printf 'MemTotal:       65536 kB\nMemFree:          10000 kB\nMemAvailable:     32768 kB\n' >"$dir/meminfo"
  printf 'processor\t: 0\nmodel name\t: a cpu\nprocessor\t: 1\n' >"$dir/cpuinfo"
  # A row is "<pid> <short name> <cpu ticks> <resident pages>". The start ticks
  # come from the pid, so two trees give the same process the same start.
  for row in "$@"; do
    read -r pid name ticks rss <<<"$row"
    mkdir -p "$dir/$pid"
    printf '%s (%s) S 1 %s %s 0 -1 1 0 0 0 0 %s 0 0 0 20 0 1 0 %s 1048576 %s 0 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0\n' \
      "$pid" "$name" "$pid" "$pid" "$ticks" "$((900 + pid - 100))" "$rss" >"$dir/$pid/stat"
  done
}

reset() {
  rm -rf "$T"/calls/* "$live" "$proc1" "$proc2"
  mkdir -p "$live"
  unit_stub
  proc_files "$proc1" 1000 '1000 100 500 8000 200 10 5 20' \
    '100 systemd 150 300' '200 tehas-cycle 0 800' '300 pi-rukas 730 1200' '400 gh 10 100'
  # A command line is never opened. It is here to be missed.
  printf '%s\n' 'gh_SecretTokenInACommandLine --repo acme/widget' >"$proc1/400/cmdline"
  T_ENV=(TEHAS_PROC_DIR="$proc1")
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

# The C1 controls, U+0080 to U+009F. In UTF-8 they are the pair 0xc2 followed by
# a byte from 0x80 to 0x9f. A bare byte range would not do: the same byte values
# are continuation bytes of characters that are not controls.
c1_bytes() { printf '%s' "$1" | LC_ALL=C grep -o "$(printf '\xc2[\x80-\x9f]')" | wc -l; }

t_case "--json gives one object: generatedAt and one key per section"
reset
files
energy
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[(.generatedAt | type), (keys | length)] | join(" ")' <<<"$t_out")" "string 9"
t_eq "$(jq -r '[.factory, .queue, .cycle, .host, .units, .energy, .console | type] | join(",")' <<<"$t_out")" \
  "object,object,object,object,array,object,object"
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

t_case "a frame with a running cycle has all seven sections"
reset
files
energy
t_run "$top --once"
t_eq "$t_rc" 0
for line in "tehas top" "factory   pass: Working on #12." "queue     1 in line, 1 building, 1 in review, 1 stopped" \
  "  12     running    Two cycles side by side" "cycle     #12 running, step develop, started 10 min ago" \
  "host      2 cpus, load 0.10 0.20 0.30" "  100  systemd" "  400  gh" \
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
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=200 LINES=24)
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
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=40 LINES=16)
t_run "$top --once"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | awk 'length > 40' | wc -l)" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 16
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=200 LINES=16)
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | awk 'length > 200' | wc -l)" 0

# stty size prints the rows first and the columns after them. A terminal that is
# not square shows which way round they were read.
t_case "the rows and the columns of the terminal are not swapped"
reset
files "$(printf '%150s' w | tr ' ' w)"
energy
t_stub stty 'printf "30 100\n"'
seq -f 'console line %g' 1 40 >"$live/console-12.txt"
T_ENV=(TEHAS_PROC_DIR="$proc1")
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$(t_calls stty)" "size"
# 30 rows: the frame holds 30 lines, not 100 and not the default 24.
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 30
# 100 columns: the longest line is cut at 100, not at 30.
t_eq "$(printf '%s\n' "$t_out" | awk 'length > 100' | wc -l)" 0
t_eq "$(printf '%s\n' "$t_out" | awk 'length > max { max = length } END { print max }')" 100
rm -f "$T/bin/stty"

t_case "a short terminal drops console lines, not the sections"
reset
files
energy
# Named lines: a bare number could match the snapshot's clock.
seq -f 'console line %g' 1 30 >"$live/console-12.txt"
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=12)
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | wc -l)" 12
for line in "factory   pass" "queue     1 in line" "cycle     #12 running" "units     3 units" \
  "energy    312.5 W" "console   #12"; do
  t_contains "$t_out" "$line"
done
t_lacks "$t_out" "console line 30"

t_case "a short terminal drops process rows first, then console lines, then unit rows"
reset
files
energy
seq -f 'console line %g' 1 30 >"$live/console-12.txt"
# The whole frame at a height that drops nothing.
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=64)
t_run "$top --once"
t_eq "$t_rc" 0
full=$(printf '%s\n' "$t_out" | wc -l)
# One row short: a process row goes, and no console line or unit row.
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=$((full - 1)))
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | wc -l)" $((full - 1))
t_lacks "$t_out" "  400  gh"
t_contains "$t_out" "  100  systemd"
t_contains "$t_out" "console line 30"
t_contains "$t_out" "tehas-dispatch.timer        enabled"
# Four rows short: every process row goes, and still no console line.
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=$((full - 4)))
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | wc -l)" $((full - 4))
t_lacks "$t_out" "  400  gh"
t_lacks "$t_out" "  100  systemd"
t_contains "$t_out" "console line 30"
t_contains "$t_out" "host      2 cpus, load"
# Eight rows short: the console lines go before any unit row is touched.
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=$((full - 8)))
t_run "$top --once"
t_eq "$(printf '%s\n' "$t_out" | wc -l)" $((full - 8))
t_lacks "$t_out" "console line 30"
t_lacks "$t_out" "console line 27"
t_contains "$t_out" "console line 26"
t_contains "$t_out" "tehas-dispatch.timer        enabled"

t_case "an escape sequence in a title or a console line does not reach the output"
reset
files "$(printf '\033[31mRed title')"
printf '%s\n' "$(printf '\033]0;evil\007 console line')" >"$live/console-12.txt"
t_run "$top --once"
t_eq "$t_rc" 0
t_eq "$(control_bytes "$t_out")" 0
t_contains "$t_out" "Red title"
t_contains "$t_out" "evil console line"

# The C1 controls, U+0080 to U+009F, introduce a control sequence on some
# terminals. They come in two shapes: bytes in the file, which the byte filter
# reads, and a \u009b escape, which is only a control once the JSON is read.
# Text that is not ASCII and is not a control still shows.
t_case "a C1 control does not reach the output, from bytes in the file"
reset
files "Red $(printf '\302\233')title with $(printf '\303\244\303\266')"
printf '%s\n' "console$(printf '\302\233') line with $(printf '\303\244')" >"$live/console-12.txt"
t_run "$top --once"
t_eq "$t_rc" 0
t_eq "$(c1_bytes "$t_out")" 0
t_contains "$t_out" "Red title with $(printf '\303\244\303\266')"
t_contains "$t_out" "console line with $(printf '\303\244')"

t_case "a C1 control does not reach the output, from a JSON escape"
reset
files
esc='Escaped \u009btitle \u00e4\u00f6'   # the escape, not yet a control character
printf '%s\n' "{\"generatedAt\":\"$(iso 210)\",\"repo\":\"acme/widget\",\"paused\":false,\"running\":[{\"number\":12,\"title\":\"$esc\"}],\"queued\":[],\"awaiting\":[],\"stopped\":[]}" >"$live/status.json"
t_run "$top --once"
t_eq "$t_rc" 0
t_eq "$(c1_bytes "$t_out")" 0
t_contains "$t_out" "Escaped title $(printf '\303\244\303\266')"

# The byte filter itself: the C1 pair goes, and so does every other control that
# is not a tab or a newline. A character that merely has a continuation byte in
# the same range - the 0x82 of a euro sign - stays.
t_case "the byte filter drops C1 controls and leaves other UTF-8 text alone"
reset
printf 'a\007b\011c\302\233d \303\244\303\266 \342\202\254 e\177f\n' >"$T/in"
printf 'ab\tcd \303\244\303\266 \342\202\254 ef\n' >"$T/want"
t_run '. "$T_ROOT/lib/top.sh"; tehas_sanitize_text <"$T/in" >"$T/out"'
t_eq "$t_rc" 0
t_eq "$(cat "$T/out")" "$(cat "$T/want")"
t_eq "$(c1_bytes "$(cat "$T/out")")" 0

# The name of the console file is text read out of running.json, and the file it
# names is opened. A name that is not a plain file name under live/ reads nothing
# at all, whatever else it might point at.
t_case "a console name that is not a plain file name reads nothing"
reset
files
printf 'A file outside live/ that must not appear\n' >"$T/home/tehas/secret.txt"
for name in '../secret' '../../secret' '..' '.secret' 'sub/console-12.txt' 'console 12.txt' 'con$sole'; do
  jq --arg c "$name" '.cycles[0].console = $c' "$live/running.json" >"$T/r" && mv "$T/r" "$live/running.json"
  t_run "$top --once"
  t_eq "$t_rc" 0
  t_contains "$t_out" "console   no console file"
  t_lacks "$t_out" "must not appear"
done
# The name a cycle actually writes still reads its file.
jq --arg c 'console-12.txt' '.cycles[0].console = $c' "$live/running.json" >"$T/r" && mv "$T/r" "$live/running.json"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "console   #12  console-12.txt  live"
t_contains "$t_out" "[step develop] @developer ran the gate"

# The frame is one jq program: the number of jq processes a redraw costs does not
# grow with the number of lines the frame draws. The wrapper counts the calls and
# then runs the real jq.
t_case "a frame costs the same jq processes whatever it holds"
reset
files
energy
jqreal=$(command -v jq)
t_stub jq 'printf "jq\n" >>"$T/calls/jqcount"; exec '"$jqreal"' "$@"'
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=200)
seq -f 'console line %g' 1 2 >"$live/console-12.txt"
rm -f "$T/calls/jqcount"
t_run "$top --once"
t_eq "$t_rc" 0
short_lines=$(printf '%s\n' "$t_out" | wc -l)
short_calls=$(wc -l <"$T/calls/jqcount")
seq -f 'console line %g' 1 60 >"$live/console-12.txt"
rm -f "$T/calls/jqcount"
t_run "$top --once"
t_eq "$t_rc" 0
long_lines=$(printf '%s\n' "$t_out" | wc -l)
long_calls=$(wc -l <"$T/calls/jqcount")
rm -f "$T/bin/jq"
# The long frame really is 58 lines longer, and both cost the same jq.
t_eq "$((long_lines - short_lines))" 58
t_eq "$long_calls" "$short_calls"
t_eq "$((long_lines > long_calls))" 1

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
T_ENV=(TEHAS_PROC_DIR="$proc1" LINES=10 COLUMNS=80)
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
T_ENV=(TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=20)
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
for line in "factory   no health file yet" "cycle     no cycle written yet" "host      no reading of /proc" \
  "units     systemctl --user gave no answer" "energy    no sample" "console   no console file"; do
  t_contains "$t_out" "$line"
done

t_case "the host section names cpus, load, cpu use, memory and the processes"
reset
files
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.host.cpus, (.host.load | join(" ")), .host.memory.totalMb, .host.memory.usedMb' <<<"$t_out")" "2
0.10 0.20 0.30
64
32"
t_eq "$(jq -r '[.host.processes[].name] | sort | join(",")' <<<"$t_out")" "gh,pi-rukas,systemd,tehas-cycle"
# Only these five things about a process, whatever the tree held beside them.
t_eq "$(jq -c '[.host.processes[] | keys] | unique' <<<"$t_out")" \
  '[["cpuPercent","memPercent","name","pid","upSeconds"]]'
t_eq "$(jq -c '[.host.processes[] | {p: (.pid | type), u: (.upSeconds | type), m: (.memPercent | type)}] | unique' <<<"$t_out")" \
  '[{"p":"number","u":"number","m":"number"}]'
# One snapshot reads the same tree twice, and a tree that says the same thing
# both times measured nothing: its CPU figures are null, not a guess from what
# a process has used since it started.
t_eq "$(jq -c '[.host.cpuPercent] + [.host.processes[].cpuPercent]' <<<"$t_out")" \
  '[null,null,null,null,null]'

t_case "a host that cannot be read is a null section and one line"
reset
files
T_ENV=(TEHAS_PROC_DIR="$T/no-such-tree")
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.host' <<<"$t_out")" "null"
t_eq "$(jq -r 'keys | length' <<<"$t_out")" 9
t_eq "$(jq -c '.reading' <<<"$t_out")" "null"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "host      no reading of /proc"
t_contains "$t_out" "cycle     #12 running"
T_ENV=(TEHAS_PROC_DIR="$proc1")

t_case "a process is named by its short name, never by its command line"
reset
files
t_run "$top --json"
t_eq "$t_rc" 0
t_lacks "$t_out" "SecretTokenInACommandLine"
t_lacks "$t_out" "--repo"
t_contains "$t_out" '"name":"gh"'
t_run "$top --once"
t_eq "$t_rc" 0
t_lacks "$t_out" "SecretTokenInACommandLine"
t_contains "$t_out" "  400  gh"

# The name is what lies between the first ( and the last ) of the stat file, so
# a name that holds brackets or quotes stays one name and reaches the snapshot
# as text. A directory that is not a pid, and a pid with no stat file, are not
# processes: only the numbers name one.
t_case "a short name holding brackets or quotes is still one name"
reset
files
proc3=$T/proc3
proc_files "$proc3" 1000 '1000 100 500 8000 200 10 5 20' \
  '100 systemd 150 300' '500 a(b)c\"d 20 100' '600 [bracket] 30 100'
mkdir -p "$proc3/notapid" "$proc3/700"
printf '%s\n' '700 (notapid) S 1 1 1 0 -1 1 0 0 0 0 1 1 0 0 20 0 1 0 900 1048576 10 0' >"$proc3/notapid/stat"
T_ENV=(TEHAS_PROC_DIR="$proc3")
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '[.host.processes[].pid] | join(",")' <<<"$t_out")" "100,500,600"
t_eq "$(jq -r '[.host.processes[].name] | join(",")' <<<"$t_out")" 'systemd,a(b)c\"d,[bracket]'
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "a(b)c"
t_lacks "$t_out" "notapid"

# A host that says nothing about its load or its memory is still a reading: the
# line says ? where a figure is missing, and holds no separator with nothing
# either side of it.
t_case "a tree with no load averages and no memory gives one line of question marks"
reset
files
proc4=$T/proc4
proc_files "$proc4" 1000 '1000 100 500 8000 200 10 5 20' '100 systemd 150 300'
rm -f "$proc4/loadavg" "$proc4/meminfo"
T_ENV=(TEHAS_PROC_DIR="$proc4")
t_run "$top --json"
t_eq "$(jq -c '[.host.load, .host.memory, .host.cpuPercent]' <<<"$t_out")" "[null,null,null]"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "host      2 cpus, load ?, cpu ?%, mem ? of ? MB"

published() { [ -f "$live/top.json" ] && echo there || echo gone; }
published_keys() { jq -r 'keys | join(",")' "$live/top.json"; }

# A sleep that records that it was asked and does nothing, in a directory of its
# own: only a case that puts it first on PATH gets it, so every other case in
# this file still sleeps for real.
nosleep() {
  mkdir -p "$T/nosleep"
  cat >"$T/nosleep/sleep" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/sleep"
EOF
  chmod +x "$T/nosleep/sleep"
}

# The same tree five seconds on: tehas-cycle used 5 s of CPU in the window, and
# the aggregate line moved by 1000 ticks of which 500 were idle - 50% over the
# host, and 100% for that one process.
proc_ahead() { # <dir>
  proc_files "$1" 1005 '1500 100 500 8500 200 10 5 20' \
    '100 systemd 160 300' '200 tehas-cycle 500 800' '300 pi-rukas 735 1200' '400 gh 12 100'
}

t_case "the snapshot carries the reading its host section was computed from"
reset
files
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '(.reading.clock == 1000), .reading.cpus, (.reading.procs | length), (.reading | type)' <<<"$t_out")" \
  "true
2
4
object"
# A tree that cannot be read gives no reading either: the next call has nothing
# to compare against and takes a pair.
T_ENV=(TEHAS_PROC_DIR="$T/no-such-tree")
t_run "$top --json"
t_eq "$(jq -c '.reading' <<<"$t_out")" "null"
T_ENV=(TEHAS_PROC_DIR="$proc1")

t_case "a snapshot given the reading of the frame before does not wait for a second one"
reset
files
nosleep
T_ENV=(TEHAS_PROC_DIR="$proc1" PATH="$T/nosleep:$T/bin:$PATH")
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_snapshot 16 >"$T/one"'
t_eq "$t_rc" 0
# Nothing to compare with: a pair of readings, and the second one a second on.
t_eq "$(t_calls sleep | tr '\n' ' ')" "1 "
t_eq "$(jq -r '.reading.clock' "$T/one")" "1000.00"
t_eq "$(jq -c '[.host.cpuPercent] + [.host.processes[].cpuPercent]' "$T/one")" \
  '[null,null,null,null,null]'
# Time passes, as it does between two frames of a view. The reading of the frame
# before is handed to the next one, and that call takes no sleep at all.
proc_ahead "$proc1"
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_snapshot 16 "$(jq -c .reading "$T/one")" >"$T/two"'
t_eq "$t_rc" 0
t_eq "$(t_calls sleep | tr '\n' ' ')" "1 "
t_eq "$(jq -r '.reading.clock' "$T/two")" "1005.00"
# Measured over the five seconds between the two readings: 50% for the host, and
# 100% for the process that used them.
t_eq "$(jq -c '.host.cpuPercent, [.host.processes[] | select(.name == "tehas-cycle") | .cpuPercent]' "$T/two")" \
  "50
[100]"
t_eq "$(jq -r '[.host.processes[] | select(.name == "tehas-cycle") | .upSeconds] | .[0]' "$T/two")" "995"
# The frame is drawn from the snapshot as it is: the extra key changes nothing
# that is printed.
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_frame "$(cat "$T/two")" 80 24'
t_contains "$t_out" "host      2 cpus, load 0.10 0.20 0.30, cpu 50%, mem 32 of 64 MB"
t_contains "$t_out" "tehas-cycle"

# Two readings a hundred seconds apart, written by hand: the sleepy one has
# used a thousand seconds of CPU in its life and one second in this window, the
# busy one forty seconds in the window and almost none in its life. Only the
# difference is shown.
t_case "CPU percent is the difference between two readings, not since it started"
reset
jq -c -n '{clock: 1000.00, cpus: 2, load: [0.1, 0.2, 0.3],
           memory: {totalKb: 65536, availKb: 32768}, cpuAll: 100000, cpuIdle: 90000,
           procs: ([[100, "sleepy", 100000, 100, 400], [200, "busy", 100, 200, 400],
                    [600, "gone", 500, 600, 10]] +
                   [range(300; 316) | [., "p" + (. | tostring), 1000, 500, 10]])}' >"$T/r1"
jq -c -n '{clock: 1100.00, cpus: 2, load: [0.1, 0.2, 0.3],
           memory: {totalKb: 65536, availKb: 32768}, cpuAll: 200000, cpuIdle: 180000,
           procs: ([[100, "sleepy", 100100, 100, 400], [200, "busy", 4100, 200, 400],
                    [700, "new", 50, 700, 10]] +
                   [range(300; 316) | [., "p" + (. | tostring), 1000 + ((. - 300) * 100), 500, 10]])}' >"$T/r2"
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_host "$(cat "$T/r2")" "$(cat "$T/r1")" >"$T/host"'
t_eq "$t_rc" 0
t_eq "$(jq -r '.cpuPercent' "$T/host")" "10"
t_eq "$(jq -r '[.processes[0].name, .processes[0].cpuPercent, (.processes | length)] | join(" ")' "$T/host")" "busy 40 15"
# The fifteen that used the most CPU in this window. sleepy is out although it
# has used more CPU than any of them in its life; so are the two processes that
# used less than the fifteenth.
t_eq "$(jq -c '[.processes[].name]' "$T/host")" \
  '["busy","p315","p314","p313","p312","p311","p310","p309","p308","p307","p306","p305","p304","p303","p302"]'
t_eq "$(jq -c '[.processes[].cpuPercent] | unique' "$T/host")" '[2,3,4,5,6,7,8,9,10,11,12,13,14,15,40]'
# A process only in the later reading and one only in the earlier are not
# measured: there is nothing to take a difference between.
t_eq "$(jq -c '[.processes[].name] | map(select(. == "new" or . == "gone"))' "$T/host")" "[]"
t_eq "$(jq -r '.processes[0].upSeconds' "$T/host")" 1098

t_case "a pid that began again is not read as one long process"
reset
jq -c -n '{clock: 1000.00, cpus: 1, load: [0, 0, 0], memory: null, cpuAll: 1000, cpuIdle: 900,
           procs: [[100, "cycle", 5000, 500, 400]]}' >"$T/r1"
jq -c -n '{clock: 1010.00, cpus: 1, load: [0, 0, 0], memory: null, cpuAll: 2000, cpuIdle: 1900,
           procs: [[100, "cycle", 50, 50000, 400]]}' >"$T/r2"
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_host "$(cat "$T/r2")" "$(cat "$T/r1")" >"$T/host"'
t_eq "$t_rc" 0
t_eq "$(jq -c '.processes' "$T/host")" "[]"

t_case "the redrawing view hands the reading of one frame to the next"
reset
files
# The live loop is the one thing a test cannot run here: it needs a terminal.
# What it does with the reading is checked in the script it runs.
t_contains "$(cat "$T_ROOT/libexec/top")" 'tehas_top_snapshot "$rows" "$prev"'
t_contains "$(cat "$T_ROOT/libexec/top")" "jq -c '.reading // empty'"
t_contains "$(cat "$T_ROOT/libexec/top")" "prev=''
"

t_case "the published file is the snapshot cut to the keys that may be published"
reset
files
energy
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$top --json"
snapshot=$t_out
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(published)" "there"
# The two snapshots are taken a second apart, so the age of the health file -
# and the moment stamped on the snapshot - differ by that second. Everything
# else is the same, and neither the console of a cycle nor the reading the host
# section was computed from is in the published file at all.
t_eq "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' "$live/top.json")" \
  "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' <<<"$snapshot")"
t_eq "$(jq -r '.host | type' "$live/top.json")" "object"
t_eq "$(published_keys)" "cycle,energy,factory,generatedAt,host,queue,units"
t_lacks "$live/top.json" "SecretTokenInACommandLine"

# What is published is a list of keys, not a list of keys to take out: two keys
# that are in the snapshot today reach the file only because they are named.
t_case "a key added to the snapshot later does not reach the published file"
reset
files
energy
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$(published_keys)" "cycle,energy,factory,generatedAt,host,queue,units"
# The same snapshot with a key no page draws, and one that holds a token: what
# comes out of the cut is what it was without them.
t_run '. "$T_ROOT/lib/top.sh"; snapshot=$(tehas_top_snapshot 16)
  extended=$(jq -c ". + {token: \"ghp_aStringThatIsASecret\", draft: {note: \"not published\"}}" <<<"$snapshot")
  tehas_top_published "$extended" >"$T/out"
  jq -c "del(.generatedAt, .factory.ageSeconds, .console, .reading)" <<<"$snapshot" >"$T/kept"'
t_eq "$t_rc" 0
t_eq "$(jq -r 'keys | join(",")' "$T/out")" "cycle,energy,factory,generatedAt,host,queue,units"
t_lacks "$T/out" "ghp_aStringThatIsASecret"
t_lacks "$T/out" "not published"
t_lacks "$T/out" "\"token\""
t_eq "$(cat "$T/kept")" "$(jq -c 'del(.generatedAt, .factory.ageSeconds)' "$T/out")"

# The cut answers nothing for a snapshot that is not one.
t_case "the published cut refuses a snapshot that is not an object"
reset
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_published "{not json}"'
t_eq "$t_rc" 1
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_published ""'
t_eq "$t_rc" 1

t_case "the publisher writes the file and changes nothing else under live/"
reset
files
before=$(cd "$live" && cksum ./* | LC_ALL=C sort)
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(cd "$live" && find . -maxdepth 1 -type f ! -name 'top.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "console-12.txt health.json running.json status.json "
t_contains "$(jq -c '.host.processes[].name' "$live/top.json")" "tehas-cycle"
t_eq "$(cd "$live" && cksum ./console-12.txt ./health.json ./running.json ./status.json | LC_ALL=C sort)" "$before"

t_case "TEHAS_TOP unset or 0 removes the file and exits 0"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(published)" "there"
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=0
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(published)" "gone"
t_instance "${T_MINIMAL[@]}"
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(published)" "gone"
# The setting governs the published file only: the terminal view works anyway.
t_run "$top --json | jq -r '.host | type'"
t_eq "$t_out" "object"

t_case "a TEHAS_TOP that is not 0 or 1 stops with 78 and leaves the file alone"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_run "$top --json"
snapshot=$t_out
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=yes
t_run "$publish"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_TOP is not valid: expected 0 or 1"
t_eq "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' "$live/top.json")" \
  "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' <<<"$snapshot")"

t_case "with no instance settings the publisher stops with 78 and writes nothing"
reset
files
mv "$TEHAS_INSTANCE_DIR/instance.env" "$T/instance.env.bak"
t_run "$publish"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$(published)" "gone"
mv "$T/instance.env.bak" "$TEHAS_INSTANCE_DIR/instance.env"

t_case "a file that cannot be written leaves the one that is there in place"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$t_rc" 0
keep=$(cat "$live/top.json")
# A directory where the temporary file goes: the write fails however the file
# modes are set, and the rename is never reached.
mkdir -p "$live/top.json.new"
t_run "$publish"
t_eq "$t_rc" 1
t_contains "$t_out" "could not write"
t_eq "$(cat "$live/top.json")" "$keep"
rmdir "$live/top.json.new"

t_case "with --every a malformed setting stops the publisher instead of looping"
reset
files
# The loop re-runs this script at every mark. Settings that are broken do not
# mend themselves between marks: the unit is told, so systemd shows it.
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=yes
t_run "$publish --every"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_TOP is not valid: expected 0 or 1"
t_eq "$(published)" "gone"

# The loop re-runs the script, and the re-run loads the settings again, which
# clears TEHAS_PROC_DIR from its own environment. What it reads has to be said
# to it, or a host section written in a loop would come from the machine that
# ran it rather than the one it was pointed at.
t_case "with --every the runs it starts read the host tree it was given"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
# One turn of the loop, stopped as soon as the file is there: what matters is
# which tree that run read, not how many snapshots it wrote. Its own output goes
# to a file, so nothing the loop leaves behind holds this one open.
t_run "$publish --every >\"$T/every.log\" 2>&1 & child=\$!; for _ in \$(seq 1 60); do [ -e \"$live/top.json\" ] && break; sleep 0.1; done; kill \$child 2>/dev/null; wait \$child 2>/dev/null; true"
t_eq "$(published)" "there"
t_eq "$(cat "$T/every.log")" ""
t_eq "$(jq -r '[.host.processes[].name] | sort | join(",")' "$live/top.json")" \
  "gh,pi-rukas,systemd,tehas-cycle"

t_case "the publisher takes no other argument"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish --watch"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehasbot top [--every]"
t_eq "$(published)" "gone"

t_done
