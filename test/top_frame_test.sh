#!/usr/bin/env bash
# Tests for tehas top: the frame --once draws, its sections, its size and what
# it lets through to the terminal.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/top_lib.sh
. "$(dirname "$0")/top_lib.sh" || exit 1

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
t_done
