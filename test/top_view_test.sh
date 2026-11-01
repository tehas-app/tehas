#!/usr/bin/env bash
# Tests for tehas top: the console a frame opens, what a redraw costs, systemd
# without an answer, the arguments, and a run that changes nothing.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/top_lib.sh
. "$(dirname "$0")/top_lib.sh" || exit 1
# The readings are the same files a second apart or not, so the first frame's
# wait for a second reading takes no time here (top_snapshot_test.sh covers it).
t_stub sleep

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

# A redraw costs no jq process, whatever the frame draws. The wrapper counts the
# calls and then runs the real jq.
t_case "a frame costs no jq process whatever it holds"
reset
files
energy
jqreal=$(command -v jq)
t_stub jq 'printf "jq\n" >>"$T/calls/jqcount"; exec '"$jqreal"' "$@"'
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=200)
seq -f 'console line %g' 1 2 >"$live/console-12.txt"
t_run "$top --once"
t_eq "$t_rc" 0
short_lines=$(printf '%s\n' "$t_out" | wc -l)
seq -f 'console line %g' 1 60 >"$live/console-12.txt"
t_run "$top --once"
t_eq "$t_rc" 0
long_lines=$(printf '%s\n' "$t_out" | wc -l)
rm -f "$T/bin/jq"
# The long frame really is 58 lines longer, and neither called jq.
t_eq "$((long_lines - short_lines))" 58
t_eq "$([ -e "$T/calls/jqcount" ] && echo called)" ""

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
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" LINES=10 COLUMNS=80)
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

# The frame fills the height and is cut to the width, so the two say what size
# it was drawn for. A long console takes the rows and a long title the width.
drawn() { printf '%s\n' "$t_out" | awk 'length > w { w = length } END { print w "x" NR }'; }

t_case "the size comes from COLUMNS and LINES, then the terminal, then 80 by 24"
reset
files "$(printf 'long title %.0s' $(seq 1 60))"
seq -f 'console line %g' 1 300 >"$live/console-12.txt"
t_stub stty 'printf "%s\n" "$*" >>"$T/calls/asked"; echo "30 100"'
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS=60 LINES=40)
t_run "$top --once"
t_eq "$(drawn)" "60x40"
t_eq "$([ -e "$T/calls/asked" ] && echo asked)" ""
# One of the two given: the terminal says the other, rows first.
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS=60 LINES=)
t_run "$top --once"
t_eq "$(drawn)" "60x30"
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS=abc LINES=40)
t_run "$top --once"
t_eq "$(drawn)" "100x40"
t_eq "$(sort -u "$T/calls/asked")" "size"
# A terminal that cannot be asked: 80 by 24.
t_stub stty 'exit 1'
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS= LINES=)
t_run "$top --once"
t_eq "$(drawn)" "80x24"
# Clamped both ways.
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS=5 LINES=1000)
t_run "$top --once"
t_eq "$(drawn)" "20x200"
rm -f "$T/bin/stty"

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
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" COLUMNS=80 LINES=20)
t_run "$top --once && $top --json >/dev/null"
t_eq "$t_rc" 0
t_eq "$(cd "$live" && cksum ./* | LC_ALL=C sort)" "$before"
t_eq "$(cd "$live" && find . -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "console-12.txt energy.json health.json running.json status.json "

t_case "the frame comes from the snapshot alone: a snapshot with nothing in it"
reset
printf '%s' '{"generatedAt":"2026-01-01T00:00:00Z"}' >"$T/snap"
t_run '"$TEHAS_CONSOLE" top frame 80 24 <"$T/snap"'
t_eq "$t_rc" 0
for line in "factory   no health file yet" "cycle     no cycle written yet" "host      no reading of /proc" \
  "units     systemctl --user gave no answer" "energy    no sample" "console   no console file"; do
  t_contains "$t_out" "$line"
done
t_done
