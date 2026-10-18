#!/usr/bin/env bash
# Tests for libexec/dispatch: kept workspaces that are removed, and cycles
# that have gone quiet.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# Kept workspaces. Issue 5's were last used eight days ago, issue 6's today.
works=$T/home/tehas/work
kept() { # <issue> <age, as touch -d takes it> [what is beside it]
  local p
  mkdir -p "$works" "$T/home/tehas/state/cycles"
  : >"$works/.tehas-workspaces"
  for p in "$works/widget-$1" "${@:3}"; do
    mkdir -p "$p"
    touch -d "$2" "$p"
  done
  touch -d "$2" "$T/home/tehas/state/cycles/$1.lock"
}
old() { kept "$1" "8 days ago" "$works/widget-$1.before-retry" "$works/widget-$1.images" "$works/widget-$1.state"; }
left() { find "$works" -mindepth 1 -maxdepth 1 ! -name .tehas-workspaces -printf '%f\n' | sort | tr '\n' ' '; }

t_case "a workspace no cycle has used for seven days goes, with what is beside it, and that is logged"
reset
rm -rf "$works"
old 5
kept 6 "1 day ago" "$works/widget-6.before-retry"
mkdir -p "$works/widget-5x" "$works/other-5"
touch -d "8 days ago" "$works/widget-5x" "$works/other-5"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "other-5 widget-5x widget-6 widget-6.before-retry "
t_contains "$t_out" "removed the kept workspace of #5: no cycle has run on it for 7 days"

t_case "a workspace whose issue's lock is held is kept, however old"
reset
rm -rf "$works"
old 5
hold 5
touch -d "8 days ago" "$works/widget-5" "$T/home/tehas/state/cycles/5.lock"
t_run "$dispatch"
release
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_lacks "$t_out" "removed the kept workspace"

t_case "a workspace moved aside recently keeps the issue's workspaces"
reset
rm -rf "$works"
old 5
touch "$works/widget-5.before-retry"
t_run "$dispatch"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

t_case "nothing goes from a directory tehas work did not mark"
reset
rm -rf "$works"
old 5
rm "$works/.tehas-workspaces"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

t_case "a dry run says what would go and removes nothing"
reset
rm -rf "$works"
old 5
t_run "$dispatch --dry-run"
t_contains "$t_out" "would remove the kept workspace of #5: no cycle has run on it for 7 days"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

t_case "a poll removes the workspaces of three issues at most"
reset
rm -rf "$works"
for n in 2 3 4 5; do kept "$n" "8 days ago"; done
t_run "$dispatch"
t_eq "$(left)" "widget-5 "
t_run "$dispatch"
t_eq "$(left)" ""
rm -rf "$works"

# A cycle on #9 whose live file was last written <age> seconds ago, and that
# took its lock <taken> seconds ago.
quiet_cycle() { # <age> <taken>
  hold 9
  mkdir -p "$T/home/tehas/live/cycles"
  printf '{"issue":9,"running":true,"updatedAt":%s}\n' "$(($(date +%s) - $1))" >"$T/home/tehas/live/cycles/9.json"
  touch -d "@$(($(date +%s) - $2))" "$T/home/tehas/state/cycles/9.taken"
}
# The issues with a quiet note on record.
notes() { find "$T/home/tehas/state/cycles" -maxdepth 1 -name '*.quiet' -printf '%f\n' 2>/dev/null | sed 's/\.quiet$//' | sort | tr '\n' ' '; }
# How many times the poll has asked that issue's unit to stop.
note_asks() { awk 'NR == 1 { print $2 }' "$T/home/tehas/state/cycles/$1.quiet" 2>/dev/null; }
# Time passes: the next ask of that issue's note is due now.
due() { # issue
  local f=$T/home/tehas/state/cycles/$1.quiet fields
  [ -f "$f" ] || return 0
  fields=$(awk 'NR == 1 { print $1, $2 }' "$f")
  printf '%s %s\n' "$fields" "$(($(date +%s) - 1))" >"$f"
}

t_case "a cycle that holds its lock and has not been heard from for 20 minutes is reported and stopped, once"
reset
t_stub fake-stop '[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #9 holds its lock but has not been heard from since "
t_contains "$t_out" "more than 1200s: stopping tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(find "$T/calls/stop.fd8" 2>/dev/null | wc -l)" 0
t_eq "$(notes)" "9 "
t_eq "$(note_asks 9)" "1"
t_run "$dispatch"
t_lacks "$t_out" "#9 holds its lock"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(notes)" "9 "
t_eq "$(note_asks 9)" "1"
release

t_case "a stop that failed is said, recorded, and asked again once the wait has passed"
reset
t_stub fake-stop '[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"; exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_contains "$t_out" "WARN: could not stop tehas-cycle@9.service (exit 1)"
t_contains "$t_out" "it holds its slot, and the poll asks again"
t_eq "$(find "$T/calls/stop.fd8" 2>/dev/null | wc -l)" 0
t_eq "$(note_asks 9)" "1"
# The wait for the next ask has not passed: this poll says nothing.
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
# The wait has passed: the same episode is said and the stop asked again.
due 9
t_run "$dispatch"
t_contains "$t_out" "the poll asks tehas-cycle@9.service to stop again"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 "
t_eq "$(note_asks 9)" "2"
release

t_case "a stop that never works is asked six times, then left to a person"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
for _ in 1 2 3 4 5 6 7; do
  due 9
  t_run "$dispatch"
done
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
t_eq "$(note_asks 9)" "6"
t_contains "$t_out" "asked 6 times to stop tehas-cycle@9.service and asks no more"
t_contains "$t_out" "systemctl --user stop tehas-cycle@9.service"
release

t_case "a stuck cycle whose last-heard time cannot be read says nothing and keeps its note"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$(notes)" "9 "
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
# Neither the live file nor <issue>.taken: nothing is known about this cycle.
rm -f "$T/home/tehas/live/cycles/9.json" "$T/home/tehas/state/cycles/9.taken"
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(notes)" "9 "
t_eq "$(note_asks 9)" "1"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
release

t_case "a file in state/cycles that is no issue's note is left alone, and says nothing"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
mkdir -p "$T/home/tehas/state/cycles"
mkdir "$T/home/tehas/state/cycles/x.quiet"
: >"$T/home/tehas/state/cycles/9.5.quiet"
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$t_out" "Is a directory"
t_lacks "$t_out" "could not remove"
t_eq "$([ -d "$T/home/tehas/state/cycles/x.quiet" ] && echo kept)" "kept"
t_eq "$([ -f "$T/home/tehas/state/cycles/9.5.quiet" ] && echo kept)" "kept"
release

t_case "a quiet note that is a link goes, and what it points at is not written"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
printf 'a person reads this\n' >"$T/target"
ln -s "$T/target" "$T/home/tehas/state/cycles/9.quiet"
t_run "$dispatch"
t_contains "$t_out" "state/cycles/9.quiet is not a note file: the poll removed it"
t_contains "$t_out" "stopping tehas-cycle@9.service"
t_eq "$(cat "$T/target")" "a person reads this"
t_eq "$(notes)" "9 "
t_eq "$(note_asks 9)" "1"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
release

t_case "a cycle heard from lately, or one that took its lock lately, is left alone"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 20 3600
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
release
# A live file left by the issue's last cycle, and a new one still starting.
quiet_cycle 86400 60
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a dry run says it would stop a quiet cycle, every time, and stops nothing"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: the cycle for #9 holds its lock but has not been heard from since "
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(notes)" ""
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
release

t_case "the note of an issue no cycle holds is gone from a later poll"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$(notes)" "9 "
# The unit stopped: no cycle holds the lock of #9 any more.
release
t_run "$dispatch --dry-run"
t_eq "$(notes)" "9 "
t_run "$dispatch"
t_eq "$(notes)" ""

# A note left by hand, for an issue that has never had a cycle here.
: >"$T/home/tehas/state/cycles/11.quiet"
t_run "$dispatch"
t_eq "$(notes)" ""

t_case "a quiet cycle that is heard from again keeps no note"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$(notes)" "9 "
# The stop was asked, and the cycle went on writing its live file.
quiet_cycle 20 3600
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(notes)" ""
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
release

t_done
