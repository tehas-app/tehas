#!/usr/bin/env bash
# Tests for tehas top: the console a frame opens, what a redraw costs, systemd
# without an answer, the arguments, and a run that changes nothing.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/top_lib.sh
. "$(dirname "$0")/top_lib.sh" || exit 1

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
t_done
