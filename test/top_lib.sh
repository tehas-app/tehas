# shellcheck shell=bash
# shellcheck disable=SC2016,SC2034  # stub bodies are expanded when the stub runs; the parts read the variables
# What the tests of tehas top share: test/top_*_test.sh source this. They
# cover the snapshot in lib/top.sh, the frame drawn from it, the command in
# libexec/top that wires the two, and the publisher in libexec/top-publish.
# Everything runs against files written here under $TEHAS_HOME/live/ and a
# tree of host numbers written here, with systemctl stubbed. Nothing here calls
# GitHub, a model or Docker, and no case reads the machine that runs it.
# The part has sourced test/lib.sh before this.

# The console that reads the host (console/src/top.rs): this checkout's own,
# named to the library in TEHAS_CONSOLE, since the checkout has none beside it.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
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
  # come from the pid: 900 + pid - 100, in ticks. So two trees give the same
  # process the same start, and a process whose start has to differ is written
  # by hand, as the brackets case does.
  for row in "$@"; do
    read -r pid name ticks rss <<<"${row//\|/ }"
    mkdir -p "$dir/$pid"
    printf '%s (%s) S 1 %s %s 0 -1 1 0 0 0 0 %s 0 0 0 20 0 1 0 %s 1048576 %s 0 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0\n' \
      "$pid" "$name" "$pid" "$pid" "$ticks" "$((900 + pid - 100))" "$rss" >"$dir/$pid/stat"
  done
}

# The rows of a process set, for proc_files: pids from the first to the last,
# each named after itself and all of them with the same start (the start ticks
# come from the pid). Each next pid has used <step> ticks more than the last,
# from <base> for the first. The fields of a row are kept apart by | so the
# whole set survives one unquoted word per row.
pid_rows() { # <first> <last> <base ticks> <step>
  local p base=$3 span=$4
  for p in $(seq "$1" "$2"); do
    printf "%d|p%d|%d|10 " "$p" "$p" "$((base + (p - $1) * span))"
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
  T_ENV=(TEHAS_CONSOLE="$T_CONSOLE" TEHAS_PROC_DIR="$proc1")
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
