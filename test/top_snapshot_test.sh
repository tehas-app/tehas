#!/usr/bin/env bash
# Tests for tehas top: the snapshot --json gives, and the host section and the
# reading it is computed from.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/top_lib.sh
. "$(dirname "$0")/top_lib.sh" || exit 1

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
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$T/no-such-tree")
t_run "$top --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.host' <<<"$t_out")" "null"
t_eq "$(jq -r 'keys | length' <<<"$t_out")" 9
t_eq "$(jq -c '.reading' <<<"$t_out")" "null"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "host      no reading of /proc"
t_contains "$t_out" "cycle     #12 running"
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1")

# The snapshot is read and put together by the console beside the commands.
# Without one there is no snapshot, and the command says what to do.
t_case "a host with no console has no snapshot and says to install"
reset
files
T_ENV=(TEHAS_CONSOLE="$T/no-console" TEHAS_PROC_DIR="$proc1")
t_run "$top --json"
t_eq "$t_rc" 1
t_contains "$t_out" "no console at $T/no-console, run install.sh"
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1")

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
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc3")
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
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc4")
t_run "$top --json"
t_eq "$(jq -c '[.host.load, .host.memory, .host.cpuPercent]' <<<"$t_out")" "[null,null,null]"
t_run "$top --once"
t_eq "$t_rc" 0
t_contains "$t_out" "host      2 cpus, load ?, cpu ?%, mem ? of ? MB"


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
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$T/no-such-tree")
t_run "$top --json"
t_eq "$(jq -c '.reading' <<<"$t_out")" "null"
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1")

t_case "a snapshot given the reading of the frame before does not wait for a second one"
reset
files
nosleep
T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1" PATH="$T/nosleep:$T/bin:$PATH")
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
t_done
