#!/usr/bin/env bash
# Tests for libexec/stop: what a cycle leaves on the host, what a stop by hand
# removes, what it keeps, what it refuses to touch, and the label it takes off
# the issue. systemctl, docker and gh are stubbed. The gh stub prints what gh
# really prints for `issue view --json labels`, an object, and applies the
# --jq filter it is given with real jq, so a filter that does not fit gh's
# output fails here as it fails on a host.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

stop_cmd=$T_ROOT/libexec/stop
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
  # SHOW is what the unit reports as its state, and KILL the process the unit's
  # death takes with it, as the process of a real cycle is taken: 0 ends nothing
  # of its own, and the lock stays held.
  t_stub systemctl 'case "$2" in
  show) printf "%s\n" "${SHOW:-active}" ;;
  stop) [ "${KILL:-1}" = 1 ] && kill "$(cat "$T/holder" 2>/dev/null)" 2>/dev/null
    : ;;
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
# gh: the labels of the issue, read from $T/labels.json as gh reads them (an
# object, with the --jq filter applied by real jq), and the label change.
# GH_VIEW_RC makes the read fail and GH_EDIT_RC makes the change fail, as a gh
# that cannot reach GitHub or that is refused the change does.
GH_LABELS=$T/labels.json
gh_stub() {
  t_stub gh 'case "$1 $2" in
  "issue view")
    [ "${GH_VIEW_RC:-0}" = 0 ] || exit "$GH_VIEW_RC"
    filter="" prev=""
    for a in "$@"; do [ "$prev" != --jq ] || filter=$a; prev=$a; done
    jq -r "$filter" "$T/labels.json" ;;
  "issue edit") [ "${GH_EDIT_RC:-0}" = 0 ] || exit "$GH_EDIT_RC" ;;
esac'
}
# The labels the stub reports for the issue: names as arguments.
gh_labels() { # <name...>
  local out="" l
  for l in "$@"; do out="$out{\"name\":\"$l\"},"; done
  printf '{"labels":[%s]}\n' "${out%,}" >"$GH_LABELS"
}
# What gh was asked to do, one call per line.
gh_views() { t_calls gh | grep '^issue view' || true; }
gh_edits() { t_calls gh | grep '^issue edit' || true; }

reset() {
  release
  rm -rf "$th" "$runs" "$T"/calls/* "$T/holder" "$T/holder-keep" "$T/gone"
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

# A cycle of <issue> that is running: its lock, held by a process the stubbed
# stop ends.
hold() { # <issue>
  mkdir -p "$state"
  : >"$state/$1.lock"
  ( exec 9>"$state/$1.lock"; flock -n 9 || exit 1; exec sleep 30 ) &
  echo "$!" >"$T/holder"
}
# The same, but the lock stays held however the unit is stopped: its holder is
# a process the stop's kill does not name, so the run stops the unit, waits for
# a lock that will not free, and leaves the cycle whole.
hold_whole() { # <issue>
  mkdir -p "$state"
  : >"$state/$1.lock"
  ( exec 9>"$state/$1.lock"; flock -n 9 || exit 1; exec sleep 30 ) &
  echo "$!" >"$T/holder-keep"
  # A holder of no cycle at all, so the stop's kill ends nothing of this case.
  : >"$T/holder"
  T_ENV=("${T_ENV[@]}" KILL=0)
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
  t_ended "$th" "$n" alice label --parked
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
# account, one line. What this host's account is, read the way libexec/stop
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
t_eq "$(t_calls systemctl | sed -n '/ stop /p' | wc -l)" 1
t_eq "$(gh_views | wc -l)" 1
t_eq "$(gh_edits)" "issue edit 5 --repo acme/widget --remove-label tehas:running"

t_case "a marker holds the time and the account, and a cycle left whole keeps it"
reset
# A host whose cycle ran and has not reported: its lock is held by a process the
# stubbed stop leaves alone, so the run stops the unit, waits 10 seconds for a
# lock that will not free, leaves every leftover where it is and says so. The
# marker the stop wrote before it asked is the record of that stop, and the case
# reads it as libexec/cycle would. The stop that clears the leftovers, and takes
# the marker with them, is the case after this one.
cycle_files 5
# The marker this case takes away is written again by the stop itself, and the
# name of its write goes as any leftover does.
rm -f "$state/5.stopped"
systemctl_live
# The lock of the cycle is held by a process the stop's kill does not end, so
# the run stops the unit, waits for a lock that will not free, and leaves every
# leftover where it is. The container is not on this host, so the stop's one
# call to systemctl is the record of its own order.
hold_whole 5
T_ENV=("${T_ENV[@]}" CONTAINERS=)
gh_labels tehas:running
began=$(date +%s)
t_run "$stop_cmd 5"
kill "$(cat "$T/holder-keep" 2>/dev/null)" 2>/dev/null
t_eq "$t_rc" 1
t_contains "$t_out" "stopped tehas-cycle@5.service"
t_contains "$t_out" "the cycle for #5 still holds its lock"
t_contains "$t_out" "nothing was removed"
# The marker of a cycle that was not cleared stays with its lock, and holds the
# two fields libexec/cycle reads: the seconds, then the account. The account is
# what the host says it is, so it is read back rather than compared with the
# name of the shell that runs the case.
if ! read -r marker_at marker_by <"$state/5.stopped" 2>/dev/null; then
  t_bad "the stop left no marker: $(ls "$state")"
else
  [[ $marker_at =~ ^[0-9]{1,10}$ ]] || t_bad "the marker's time is not seconds: '$marker_at'"
  [[ $marker_by =~ ^[A-Za-z0-9._-]{1,64}$ ]] ||
    t_bad "the marker names no single account: '$marker_by'"
  [ "$marker_by" = "$whoami_here" ] ||
    t_bad "the marker names '$marker_by', not $whoami_here"
  [ "$((marker_at - began))" -ge -5 ] && [ "$((marker_at - began))" -le 30 ] ||
    t_bad "the marker's time $marker_at is not the stop's, which began at $began"
fi
# The marker went before the ask: the unit is asked for once, and only after
# the name had been written.
calls=$(t_calls systemctl)
[ "$(sed -n '/ stop /p' <<<"$calls" | wc -l)" = 1 ] || t_bad "the unit was not stopped once: $calls"
# And a cycle left whole keeps every leftover it had but the one name of the
# write this stop made and moved into the marker.
t_eq "$(leftovers 5 | wc -w)" "$((LEFTOVERS_N - 1))"

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

t_case "a run that finds nothing at all says so, and writes and removes no marker"
reset
# Nothing live, nothing left: one line and no call made on this host beyond
# what reads it.
systemctl_idle
T_ENV=(CONTAINERS=)
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$(tail -n 1 <<<"$t_out")" "nothing to stop for #5"
t_eq "$(stopped_marker 5)" ""

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
# free, and clears the host.
( exec 9>"$state/5.lock"; flock -n 9 || exit 1; exec sleep 30 ) &
echo "$!" >"$T/holder"
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
t_eq "$(gh_edits)" "issue edit 5 --repo acme/widget --remove-label tehas:running"
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
t_eq "$(gh_edits)" "issue edit 5 --repo acme/widget --remove-label tehas:running"

t_case "a cycle that still holds its lock after the stop is left whole, and said, with no label changed"
reset
cycle_files 5
# The cycle's own marker stays, and this stop writes none of its own: its lock
# is held by a process the stop's kill does not name, so the marker beside it is
# neither written nor taken.
mkdir -p "$state"
systemctl_live
hold_whole 5
# Its container is on this host, and is not taken while the lock is held.
T_ENV=("${T_ENV[@]}" CONTAINERS=tehas-work-5)
began=$(date +%s)
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
kill "$(cat "$T/holder-keep" 2>/dev/null)" 2>/dev/null
# Every leftover of the cycle is where it was, the marker of this stop among
# them, and the name of its write is gone into that marker.
t_eq "$(leftovers 5 | wc -w)" "$((LEFTOVERS_N - 1))"
t_contains "$t_out" "still holds its lock"
t_eq "$(removed_containers)" ""
t_eq "$([ -e "$works/widget-5" ] && echo kept)" "kept"
# Nothing was cleared, so the issue was not touched either.
t_eq "$(t_calls gh)" ""
# The wait for the lock is bounded: it did not run on.
[ "$(($(date +%s) - began))" -lt 30 ] || t_bad "it waited $(($(date +%s) - began))s for a lock that would not free"

t_case "a cycle that holds its lock with no unit behind it is left whole too, at once"
reset
cycle_files 5
# As above: the lock is held, so nothing is cleared.
hold_whole 5
T_ENV=(CONTAINERS=)
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
kill "$(cat "$T/holder-keep" 2>/dev/null)" 2>/dev/null
t_eq "$(leftovers 5 | wc -w)" "$((LEFTOVERS_N - 1))"
t_contains "$t_out" "still holds its lock"
t_eq "$(t_calls gh)" ""

t_case "a lock that is no lock file stops it, and what it points at is not touched"
reset
cycle_files 5
rm -f "$state/5.lock"
printf 'a person reads this\n' >"$T/target"
ln -s "$T/target" "$state/5.lock"
systemctl_idle
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "5.lock is not a lock file"
t_eq "$(cat "$T/target")" "a person reads this"
t_eq "$([ -e "$works/widget-5" ] && echo kept)" "kept"
t_eq "$(t_calls gh)" ""

t_case "a container that will not be removed is said, and the rest still goes, but the label stays"
reset
cycle_files 5
systemctl_idle
t_stub docker 'case "$1 $2" in
"ps -a") printf "tehas-work-5\n" ;;
"rm -f") echo "cannot remove container: read-only filesystem" >&2; exit 1 ;;
esac'
T_ENV=(CONTAINERS=tehas-work-5)
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "could not remove the container tehas-work-5"
t_eq "$(leftovers 5)" ""
# The half that failed was the cycle's own: with a container still working on
# with the bot's token, tehas:running stays where it is.
t_eq "$(t_calls gh)" ""

t_case "no instance settings: 78, nothing removed and nothing called"
reset
t_no_instance
cycle_files 5
t_run "$stop_cmd 5"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
t_eq "$(t_calls systemctl)" ""

t_case "an issue that is not a number: 64, before anything is read"
reset
cycle_files 5
systemctl_live
docker_stub
for arg in 0 5a -5 12345678901 "" "''"; do
  t_run "$stop_cmd $arg"
  t_eq "$t_rc" 64
  t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
done
t_eq "$(t_calls systemctl)" ""
t_eq "$(t_calls docker)" ""
t_run "$stop_cmd 5 6"
t_eq "$t_rc" 64
t_contains "$t_out" "one issue number at most"
t_run "$stop_cmd 5 --keep"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --keep"
t_eq "$(t_calls gh)" ""

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

t_case "tehas:running is on the issue: one read and one edit take it off, as the last step"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
gh_labels bug tehas:running documentation
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$(gh_views)" "issue view 5 --repo acme/widget --json labels --jq .labels[].name"
t_eq "$(gh_edits)" "issue edit 5 --repo acme/widget --remove-label tehas:running"
t_contains "$t_out" "removed tehas:running from #5"
# It is the issue's half, so it comes after the host's: the last line said.
t_eq "$(tail -n 1 <<<"$t_out")" "removed tehas:running from #5"
# No other label, and nothing written on the issue.
t_lacks "$(t_calls gh)" "--add-label"
t_lacks "$(t_calls gh)" "issue comment"

t_case "tehas:running is not on the issue: the read is asked for, no change is, and the host is cleared all the same"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
gh_labels bug documentation tehas:refreshing tehas:awaiting-merge
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$(gh_views | wc -l)" 1
t_eq "$(gh_edits)" ""
t_eq "$(leftovers 5)" ""
t_lacks "$t_out" "tehas:running"
t_lacks "$t_out" "WARN"

t_case "the labels cannot be read: WARN, exit 1, and everything this host removed is still gone"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5 GH_VIEW_RC=1)
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: the labels of #5 could not be read"
t_eq "$(leftovers 5)" ""
t_eq "$(record 5)" "json manifest.json runner_exit status"
t_eq "$(removed_containers)" "tehas-work-5 "
t_eq "$(gh_edits)" ""

t_case "the label cannot be taken off: WARN, exit 1, and the host is still clear"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5 GH_EDIT_RC=1)
gh_labels tehas:running
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: tehas:running could not be taken off #5"
t_eq "$(leftovers 5)" ""
t_eq "$(gh_edits)" "issue edit 5 --repo acme/widget --remove-label tehas:running"

t_case "a cycle that took the lock again while the host was cleared keeps its label, and no edit is asked for"
reset
cycle_files 5
systemctl_idle
T_ENV=(CONTAINERS=tehas-work-5)
gh_labels tehas:running
# The next cycle of #5 starts between the host's half and the issue's: the read
# is answered, and the stub then holds the lock as tehasbot cycle does. The
# label of a cycle that runs is another cycle's label, so it stays.
t_stub gh 'case "$1 $2" in
"issue view")
  filter="" prev=""
  for a in "$@"; do [ "$prev" != --jq ] || filter=$a; prev=$a; done
  jq -r "$filter" "$T/labels.json"
  lock=$TEHAS_HOME/state/cycles/5.lock
  rm -f "$T/held"
  ( exec 9>"$lock"; flock -n 9 || exit 1; : >"$T/held"; exec sleep 30 ) </dev/null >/dev/null 2>&1 &
  echo "$!" >"$T/holder"
  for try in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
    [ -e "$T/held" ] && break
    sleep 0.1
  done
  [ -e "$T/held" ] || echo "the stub could not hold the lock" >&2 ;;
"issue edit") echo "the label of a running cycle was asked for" >&2; exit 1 ;;
esac'
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: a cycle for #5 holds its lock again, so tehas:running stays on the issue"
t_eq "$(gh_views | wc -l)" 1
t_eq "$(gh_edits)" ""
# The host's half went clean; what is left is the new cycle's own lock.
t_eq "$(leftovers 5)" "$state/5.lock"
t_eq "$(record 5)" "json manifest.json runner_exit status"
release

t_case "the read fits what gh really prints: gh's labels are an object, so a filter that indexes it as an array is a failure, not an empty answer"
reset
gh_labels tehas:running
t_run 'gh issue view 5 --repo acme/widget --json labels --jq ".labels[].name"'
t_eq "$t_out" "tehas:running"
t_run 'gh issue view 5 --repo acme/widget --json labels --jq ".[].name" >/dev/null 2>&1 || echo refused'
t_eq "$t_out" "refused"

# --- The cycle's record on the state store ------------------------------------
# A store at the installed path that notes, for each call, whether the lock
# file of #5 was there, and runs the real store.
store_notes() {
  rm -f "$th/libexec/store"
  printf '%s\n' '#!/usr/bin/env bash' \
    "printf '%s lock=%s\\n' \"\$1\" \"\$([ -e $(t_q "$state/5.lock") ] && echo there || echo gone)\" >>$(t_q "$T/calls/store")" \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$th/libexec/store"
  chmod 755 "$th/libexec/store"
}

t_case "a cycle killed outright has its record ended as stopped, before its lock file goes"
reset
# Its start is on record and never reported: no unit, no lock held.
t_started "$th" 5 --by-hand
store_notes
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_contains "$t_out" "ended the record of the cycle of #5 as stopped"
t_eq "$(t_row cycle-show 5 -- '[(.cycle.ended_at != null), .cycle.end_reason] | tostring')" '[true,"stopped"]'
t_eq "$(grep '^cycle-end' "$T/calls/store")" "cycle-end lock=there"
t_eq "$(find "$state/5.lock" 2>/dev/null | wc -l)" 0
# No history is written for it: tehas stop writes none.
t_eq "$(t_row history-show 5 -- '.rows | length')" 0

t_case "a second stop finds nothing to do and asks the store to end nothing"
rm -f "$T"/calls/*
t_run "$stop_cmd 5"
t_eq "$t_rc" 0
t_eq "$t_out" "nothing to stop for #5"
t_eq "$(grep -c '^cycle-end' "$T/calls/store" 2>/dev/null)" 0

t_case "a record that cannot be ended leaves the lock file and the rest, says so, and exits 1"
reset
cycle_files 5
t_started "$th" 5 --by-hand
rm -f "$state/5.stopped"
t_store_answers cycle-end 1 'store: busy #5: the database is locked'
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "the record of #5 could not be ended as stopped (store: busy #5: the database is locked), so nothing was removed. Run this again."
t_eq "$(find "$state/5.lock" | wc -l)" 1
t_eq "$(find "$works/widget-5" -maxdepth 0 | wc -l)" 1
t_eq "$(gh_edits)" ""
t_eq "$(t_row cycle-show 5 -- .cycle.ended_at)" null

t_case "a stop spends the retry a provider outage left owed, and the dispatcher starts nothing on its own"
reset
t_ended "$th" 5 alice label --provider --parked
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

t_case "a park that cannot be cleared leaves the lock file and the rest, says so, and exits 1"
for verb in question-answered park-retry; do
  reset
  cycle_files 5
  rm -f "$state/5.stopped"
  t_store_answers "$verb" 1 'store: busy #5: the database is locked'
  t_run "$stop_cmd 5"
  t_eq "$t_rc" 1
  t_contains "$t_out" "could not be"
  t_contains "$t_out" "(store: busy #5: the database is locked), so nothing was removed. Run this again."
  t_eq "$(find "$state/5.lock" | wc -l)" 1
  t_eq "$(find "$works/widget-5" -maxdepth 0 | wc -l)" 1
  t_eq "$(gh_edits)" ""
done
t_eq "$(t_row park-show 5 -- '.park.retries')" 0

t_case "a park that cannot be read changes nothing: 1 when the store is busy, 78 when it cannot be used"
reset
cycle_files 5
t_store_answers park-show 1 'store: busy #5: the database is locked'
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_contains "$t_out" "the park of #5 could not be read from the state store: store: busy #5"
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
t_store_answers park-show 78 'store: corrupt: the file is not a database'
t_run "$stop_cmd 5"
t_eq "$t_rc" 78
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
t_eq "$(t_calls systemctl | grep -c ' stop ')" 0

t_case "a record that cannot be read changes nothing: 1 when the store is busy, 78 when it is not there"
reset
cycle_files 5
t_store_answers cycle-show 1 'store: busy #5: the database is locked'
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
rm -f "$th/libexec/store"
t_run "$stop_cmd 5"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store is not installed (run install.sh)"
t_eq "$(leftovers 5 | wc -w)" "$LEFTOVERS_N"
t_eq "$(t_calls systemctl | grep -c ' stop ')" 0

t_done
