#!/usr/bin/env bash
# Tests for tehas stop: what a cycle leaves on the host, what a stop by hand
# removes, what it keeps and what it refuses to touch, end to end through the
# console's wiring. systemctl, docker and gh are stubbed. The cases that reach
# no GitHub are in console/tests/stop.rs, and the cases of the running mark
# (read, taken off, kept, and what could not be done) on the ports' fake in
# console/ops/src/stop.rs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# The stop is a verb of the console (console/ops/src/stop.rs), found beside the
# command on a host: TEHAS_CONSOLE names the console of this checkout instead.
t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
stop_cmd="$T_ROOT/bin/tehas stop"
th=$T/home/tehas
state=$th/state/cycles
works=$th/work
runs=$T/home/.pi/agent/ensemble-runs
# What a stop takes away, and what it leaves as the cycle's record. The two
# lists are here so a case can pin both sides of the line. The park of the
# cycle is on the state store: a stop takes its question off the record and
# spends its retry, and the park stays as the cycle's record. The sibling name of
# the nostart note goes with the note: a write of it is made beside it and moved
# into place, and tehas work clears both when a cycle starts.
GO=(lock cid "done" stalled timed stopped stopped.new oom forks nostart nostart.new progress.json result.json)
KEEP=(json manifest.json runner_exit status)

# systemctl: what the unit is doing ($UNIT_STATE), and the stop itself, which
# ends the process holding the issue's lock, as a unit that stopped does.
systemctl_live() {
  t_stub systemctl 'case "$2" in
  show) printf "%s\n" "${SHOW:-active}" ;;
  stop) kill "$(cat "$T/holder" 2>/dev/null)" 2>/dev/null; : ;;
esac'
}
# A unit that is not running, and a stop that would do nothing.
systemctl_idle() { t_stub systemctl 'case "$2" in show) printf "inactive\n" ;; esac'; }
# The containers on the host, one name per line ($CONTAINERS).
docker_stub() {
  t_stub docker 'case "$1 $2" in
  "ps -a") for c in ${CONTAINERS:-}; do [ -e "$T/gone/$c" ] || printf "%s\n" "$c"; done ;;
  "rm -f") mkdir -p "$T/gone"; : >"$T/gone/$3"; printf "%s\n" "$3" ;;
esac'
}
# gh: the issue, read from $T/issue.json, and the label change, which
# answers nothing.
GH_ISSUE=$T/issue.json
gh_stub() {
  t_stub gh 'case "$1 $2" in
  "api repos/"*) cat "$T/issue.json" ;;
esac'
}
# The issue the stub reports, with these labels: names as arguments.
gh_labels() { # <name...>
  local out="" l
  for l in "$@"; do out="$out{\"name\":\"$l\"},"; done
  printf '{"number":5,"title":"t","state":"open","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","html_url":"https://example.com/5","labels":[%s]}\n' "${out%,}" >"$GH_ISSUE"
}
# What gh was asked to do, one call per line.
gh_views() { t_calls gh | grep '^api repos/' || true; }
gh_edits() { t_calls gh | grep '^api -X' || true; }

reset() {
  release
  rm -rf "$th" "$runs" "$T"/calls/* "$T/holder" "$T/gone" "$T/bin/sleep"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
  # The state store with nothing on record.
  t_store_home "$th"
  systemctl_idle
  docker_stub
  gh_stub
  # An issue with no labels: a stop that stops nothing on this host is then a
  # run that asks GitHub for nothing to change.
  gh_labels
}

# Whether the holder of an issue's lock took it: the marker the holder writes
# just after flock succeeds. It is not an underscore name on purpose: the real
# `tehas held <issue>` asks the state store for an issue and is not this.
real_held() { # <issue>
  [ -e "$state/$1.held" ]
}
# A cycle of <issue> that is running: its lock, held by a process the stubbed
# stop ends. The holder says so by a marker once flock has the lock, and these
# two wait for it: a holder that has not locked yet is a cycle the stop finds no
# lock for, and a case that read it then would pass for the wrong reason.
hold() { # <issue>
  mkdir -p "$state"
  : >"$state/$1.lock"
  rm -f "$state/$1.held"
  (
    exec 9>"$state/$1.lock"
    flock -n 9 || exit 1
    : >"$state/$1.held"
    exec sleep 30
  ) </dev/null >/dev/null 2>&1 &
  echo "$!" >"$T/holder"
  t_wait_held "$1"
}
# Wait for the marker of <issue>'s holder: ten seconds of looks, which is what a
# fork and an flock cost a loaded machine many times over.
t_wait_held() { # <issue>
  local _look
  for _look in $(seq 1 100); do
    real_held "$1" && return 0
    command -p sleep 0.1
  done
  t_bad "the holder of the lock of #$1 never wrote its marker"
}
release() {
  local pid
  pid=$(cat "$T/holder" 2>/dev/null) || return 0
  [ -n "$pid" ] && kill "$pid" 2>/dev/null
  return 0
}

# Every file a cycle of <issue> leaves on the host, with its workspace.
cycle_files() { # <issue>
  local n=$1 ext
  mkdir -p "$state" "$th/live" "$works" "$runs/issue-$n"
  # The marker holds the seconds and the account, not the "<issue> <ext>" line
  # the other notes hold.
  for ext in "${GO[@]}" "${KEEP[@]}"; do printf '%s\n' "$n $ext" >"$state/$n.$ext"; done
  printf '%s %s\n' "$(date +%s)" "$whoami_here" >"$state/$n.stopped"
  # Its last cycle, on alice's label, parked with the question 42<issue> on record.
  t_ended "$th" "$n" alice approved --parked
  t_park "$th" "$n" failed "42$n"
  printf 'console line\n' >"$th/live/console-$n.txt"
  printf 'transcript\n' >"$runs/issue-$n/a-1-coder.json"
  mkdir -p "$works/widget-$n" "$works/widget-$n.state" "$works/widget-$n.images"
  printf 'the work\n' >"$works/widget-$n/README.md"
  : >"$works/widget-$n.before-retry"
  : >"$works/.tehas-workspaces"
}
# What this issue leaves that a stop should take, still on the host. The count of
# it is what a case says a stop left behind, so that an entry added to GO has to
# be removed by a stop for those cases to keep passing.
leftovers() { # <issue>
  local n=$1 out="" ext f
  for ext in "${GO[@]}"; do [ -e "$state/$n.$ext" ] && out="$out $state/$n.$ext"; done
  # A park that still asks for something: its question, or its retry.
  [ "$(t_row park-show "$n" -- '.park != null and (.park.question != null or .park.retries == 0)')" != true ] ||
    out="$out park-of-$n"
  for f in "$th/live/console-$n.txt" "$runs/issue-$n" \
    "$works/widget-$n" "$works/widget-$n.state" "$works/widget-$n.before-retry" "$works/widget-$n.images"; do
    [ -e "$f" ] && out="$out $f"
  done
  printf '%s\n' "${out# }"
}
# The number a case says a stop leaves behind: the notes of GO, the park on
# record, and the six paths beside them. It is stated here, not read back from the lists,
# so a note added to GO without being named here fails the cases that say a stop
# took everything a cycle left. The marker of a stop and the sibling name of its
# write are the two last of them, and are removed on the same terms as any other.
# The two names of the marker are the two last of them, and go on the same terms
# as any other: the marker of a stop keeps the line it was written with, so a
# case that means a marker of its own says the name here.
LEFTOVERS_N=20
# The record files of this issue that a stop keeps, still on the host.
record() { # <issue>
  local n=$1 out="" ext
  for ext in "${KEEP[@]}"; do [ -e "$state/$n.$ext" ] && out="$out $ext"; done
  printf '%s\n' "${out# }"
}
# The containers the stub was asked to remove.
removed_containers() { t_calls docker | sed -n 's/^rm -f //p' | tr '\n' ' '; }

# The marker of a stop, as libexec/cycle reads it: the epoch seconds and the
# account, one line. What this host's account is, read the way the stop
# reads it, so no case names a user the host does not have.
whoami_here=$(id -un)
# The marker of a stop, when the stop wrote one; the sibling name of its write
# is asked for by name, never by a glob.
stopped_marker() { cat "$state/$1.stopped" 2>/dev/null || true; }

t_case "a running cycle: its unit is stopped, its container and every leftover go, its label comes off, and the record stays"
reset
cycle_files 5
hold 5
systemctl_live
T_ENV=(UNIT_STATE=active CONTAINERS="tehas-work-5 tehas-work-51")
gh_labels tehas:running bug
began=$(date +%s)
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
[ "$(($(date +%s) - began))" -lt 30 ] || t_bad "the stop took $(($(date +%s) - began))s"
t_eq "$(leftovers 5)" ""
t_eq "$(record 5)" "json manifest.json runner_exit status"
t_eq "$(removed_containers)" "tehas-work-5 "
t_contains "$t_out" "stopped tehas-cycle@5.service"
t_contains "$t_out" "removed the container tehas-work-5"
t_contains "$t_out" "removed state/cycles/5.lock"
t_contains "$t_out" "removed state/cycles/5.cid"
t_contains "$t_out" "took the park question of #5 off the record"
t_contains "$t_out" "spent the retry of the park of #5"
t_eq "$(t_row question-known 5 --id 425 -- .recorded)" false
t_contains "$t_out" "removed live/console-5.txt"
t_contains "$t_out" "removed ~/.pi/agent/ensemble-runs/issue-5"
t_contains "$t_out" "removed work/widget-5"
t_contains "$t_out" "removed work/widget-5.state"
t_contains "$t_out" "removed work/widget-5.images"
t_contains "$t_out" "removed work/widget-5.before-retry"
t_contains "$t_out" "removed tehas:running from #5"
# The issue's half comes after the host's: the last line said.
t_eq "$(tail -n 1 <<<"$t_out")" "removed tehas:running from #5"
t_eq "$(t_calls systemctl | sed -n '/ stop /p' | wc -l)" 1
t_eq "$(gh_views)" "api repos/acme/widget/issues/5"
# Nothing else is asked of GitHub: no other label, no comment.
t_eq "$(t_calls gh | wc -l)" 2
t_eq "$(gh_edits)" "api -X DELETE repos/acme/widget/issues/5/labels/tehas%3Arunning"

t_case "the marker of a stop goes with the leftovers of the cycle it ended"
reset
# The cycle whose unit the stub ends: its report is over, its lock is free, and
# what the stop wrote goes with everything else it left, so a cycle started
# later on this issue reads none of an earlier stop as its own.
cycle_files 5
hold 5
systemctl_live
T_ENV=(CONTAINERS=)
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "stopped tehas-cycle@5.service"
t_contains "$t_out" "removed state/cycles/5.stopped"
t_eq "$(stopped_marker 5)" ""
t_eq "$(leftovers 5)" ""

t_case "a container with no unit and no lock behind it is removed, and no marker is written for it"
reset
# The marker says a person ended a cycle. A leftover container whose cycle left
# this host long ago is no cycle, so this run asks for no stop and writes none.
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "removed the container tehas-work-5"
t_eq "$(stopped_marker 5)" ""
t_eq "$(leftovers 5)" ""

t_case "a marker left by an earlier stop is said as a leftover, and goes"
reset
cycle_files 5
rm -f "$state/5.stopped.new"
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
began=$(date +%s)
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$(stopped_marker 5)" ""
t_contains "$t_out" "removed state/cycles/5.stopped"

t_case "a run that stops nothing writes no marker, and takes one that is there"
reset
# No unit, no lock: this run asks for no stop, so it writes no marker of its
# own. A marker an earlier stop left still goes, as every leftover does.
systemctl_idle
T_ENV=(CONTAINERS=)
mkdir -p "$state"
printf '%s %s\n' "$(date +%s)" "$whoami_here" >"$state/5.stopped"
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "removed state/cycles/5.stopped"
t_eq "$(stopped_marker 5)" ""
t_eq "$(t_calls systemctl | sed -n '/ stop /p' | wc -l)" 0

t_case "a marker that cannot be written is said, and the stop goes through all the same"
reset
cycle_files 5
# The marker of the cycle goes here, and the directory below stands in its
# place: the write of the stop is made through the name beside it and moved
# onto a name that is a directory, which no move can replace.
rm -f "$state/5.stopped"
mkdir -p "$state"
# A cycle whose lock the stubbed stop frees and whose unit it stops: the holder
# is the process the stop's kill ends, so the run stops the unit, finds the lock
# free, and clears the host. hold cannot stand here: the case makes the marker's
# name a directory after the holder takes the lock, and it is the lock of #5 the
# holder holds, so the marker of #5 is the one name that has to stay clear until
# the holder has it.
( exec 9>"$state/5.lock"; flock -n 9 || exit 1; : >"$state/5.held"; exec sleep 30 ) \
  </dev/null >/dev/null 2>&1 &
echo "$!" >"$T/holder"
# The real sleep, asked for before the case stubs anything of its own: this stop
# naps while the lock of #5 never frees, and the nap is shortened by the stub
# below, not by this wait.
for _look in $(seq 1 100); do
  [ -e "$state/5.held" ] && break
  command -p sleep 0.1
done
t_eq "$([ -e "$state/5.held" ] && echo held)" held
systemctl_live
mkdir "$state/5.stopped"
T_ENV=(UNIT_STATE=active CONTAINERS=)
gh_labels tehas:running
began=$(date +%s)
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "state/cycles/5.stopped could not be written"
t_contains "$t_out" "stopped tehas-cycle@5.service"
t_contains "$t_out" "removed tehas:running from #5"
t_eq "$(t_calls systemctl | sed -n '/ stop /p' | wc -l)" 1
# The cycle was stopped and its host cleared all the same. The name that could
# not be written is taken, and its sibling name is taken too: a marker of this
# issue is the one name, and no part of a write is left for a later cycle. The
# count of what a stop takes is the one of every other case: this one made the
# marker's name a directory before the run, so it is not a file to count.
t_eq "$([ -e "$state/5.stopped" ] && echo kept)" ""
t_eq "$([ -e "$state/5.stopped.new" ] && echo kept)" ""

t_case "stopping #5 touches nothing of #51 or #55, and its label is the only one gh is asked about"
reset
cycle_files 5
cycle_files 51
cycle_files 55
systemctl_idle
T_ENV=(CONTAINERS="tehas-work-5 tehas-work-51 tehas-work-55")
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$(leftovers 5)" ""
t_eq "$(wc -w <<<"$(leftovers 51)")" "$LEFTOVERS_N"
t_eq "$(wc -w <<<"$(leftovers 55)")" "$LEFTOVERS_N"
t_eq "$(removed_containers)" "tehas-work-5 "
t_eq "$(gh_edits)" "api -X DELETE repos/acme/widget/issues/5/labels/tehas%3Arunning"
t_lacks "$(t_calls gh)" "51"
t_lacks "$(t_calls gh)" "55"
t_lacks "$t_out" "51"
t_lacks "$t_out" "55"

t_case "--keep-workspace keeps the workspace and what is beside it, and takes the rest"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
t_run "$stop_cmd 5 --keep-workspace"
t_eq "$t_rc" 0
t_eq "$([ -d "$works/widget-5" ] && echo kept) $([ -e "$works/widget-5/README.md" ] && echo read)" "kept read"
t_eq "$([ -e "$works/widget-5.state" ] && echo kept) $([ -e "$works/widget-5.images" ] && echo kept) $([ -e "$works/widget-5.before-retry" ] && echo kept)" "kept kept kept"
t_eq "$(leftovers 5)" "$works/widget-5 $works/widget-5.state $works/widget-5.before-retry $works/widget-5.images"
t_eq "$(wc -w <<<"$(leftovers 5)")" 4
t_contains "$t_out" "spent the retry of the park of #5"
t_contains "$t_out" "removed state/cycles/5.result.json"
t_lacks "$t_out" "removed work/widget-5"

t_case "the container of a cycle that left no marker goes too"
reset
mkdir -p "$state"
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-9)
t_run "$stop_cmd 9"
t_eq "$t_rc" 0
t_contains "$t_out" "removed the container tehas-work-9"

t_case "a second run finds nothing to do, says so, changes nothing, and asks GitHub for nothing"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$t_out" "nothing to stop for #5"
t_eq "$(t_calls systemctl | sed -n '/ stop /p' | wc -l)" 0
t_eq "$(removed_containers)" "tehas-work-5 "
# One read for the first run, none for the second: a host with nothing to clear
# is not sent to GitHub.
t_eq "$(gh_views | wc -l)" 1
t_eq "$(gh_edits)" "api -X DELETE repos/acme/widget/issues/5/labels/tehas%3Arunning"

t_case "no instance settings: 78, nothing removed and nothing called"
reset
t_no_instance
cycle_files 5
t_run "$stop_cmd 5"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
t_eq "$(t_calls systemctl)" ""

t_case "help prints the header and does nothing"
reset
cycle_files 5
systemctl_live
docker_stub
for flag in -h --help; do
  t_run "$stop_cmd $flag"
  t_eq "$t_rc" 0
  t_contains "$t_out" "Usage:"
  t_contains "$t_out" "tehas stop <issue> --keep-workspace"
  t_lacks "$t_out" "set -uo pipefail"
done
t_run "$T_ROOT/bin/tehas help stop"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas stop <issue>"
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
t_eq "$(t_calls systemctl)" ""

t_case "help among the arguments is help, with no settings to load"
reset
t_no_instance
t_run "$stop_cmd 5 --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "no instance.env"
t_run "$stop_cmd 5 6 --help"
t_eq "$t_rc" 64
t_contains "$t_out" "one issue number at most"

t_case "it names only the paths of the issue it was given, never a glob"
reset
cycle_files 5
mkdir -p "$works/widget-5x" "$works/other-5"
printf 'x\n' >"$state/5.5.lock"
systemctl_idle
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$([ -e "$works/widget-5x" ] && echo x) $([ -e "$state/5.5.lock" ] && echo dot)" "x dot"
t_eq "$(record 5)" "json manifest.json runner_exit status"

# --- The cycle's record on the state store ------------------------------------
t_case "a cycle killed outright has its record ended as stopped, before its lock file goes"
reset
# Its start is on record and never reported: no unit, no lock held, and the
# lock file it left.
t_started "$th" 5 --by-hand
mkdir -p "$state"
: >"$state/5.lock"
# The cycle-end is seen as it happens: a line with no times left answers
# nothing, so the real store ends the record, but the seam still counts the
# call in store-answers.n. That count is a symlink to the lock file, so it is
# written into the lock file's inode when the lock file is there at the
# cycle-end, and into a new file when it is gone. lock-seen is a second name of
# that inode, kept after the stop removes the lock file.
t_store_answers cycle-end 0 unused 0
ln "$state/5.lock" "$T/lock-seen"
ln -s "$state/5.lock" "$T/store-answers.n"
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "ended the record of the cycle of #5 as stopped"
t_eq "$(t_row cycle-show 5 -- '[(.cycle.ended_at != null), .cycle.end_reason] | tostring')" '[true,"stopped"]'
# The end comes first: the lock file was there at the cycle-end, and is gone.
t_eq "$(cat "$T/lock-seen")" 1
t_eq "$(find "$state/5.lock" 2>/dev/null | wc -l)" 0
rm -f "$T/lock-seen" "$T/store-answers.n"
# Its one line of history says it was stopped (T11.3).
t_eq "$(t_row history-show 5 -- '[.rows[].status] | tostring')" '["stopped"]'

t_case "a stop spends the retry a provider outage left owed, and the dispatcher starts nothing on its own"
reset
t_ended "$th" 5 alice approved --provider --parked
t_park "$th" 5 provider-outage
t_eq "$(t_row park-show 5 -- '.park.automatic')" true
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "spent the retry of the park of #5"
t_lacks "$t_out" "park question"
t_eq "$(t_row park-show 5 -- '[.park.automatic, .park.retries] | tostring')" '[false,1]'
t_eq "$(t_row park-show --automatic -- '.parks | length')" 0
t_run "$stop_cmd 5"
t_eq "$t_out" "nothing to stop for #5"

t_done
