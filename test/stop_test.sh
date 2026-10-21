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
# lists are here so a case can pin both sides of the line.
GO=(lock taken cid park requeued "done" stalled timed quiet oom forks branch image progress.json result.json)
KEEP=(json history.jsonl by settings manifest.json approved.json rukas runner_exit status)

# systemctl: what the unit is doing ($UNIT_STATE), and the stop itself, which
# ends the process holding the issue's lock, as a unit that stopped does.
systemctl_live() {
  t_stub systemctl 'case "$2" in
  show) printf "active\n" ;;
  stop) kill "$(cat "$T/holder" 2>/dev/null)" 2>/dev/null ;;
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
  for ext in "${GO[@]}" "${KEEP[@]}"; do printf '%s\n' "$n $ext" >"$state/$n.$ext"; done
  printf '42\n' >"$th/state/park-$n.id"
  printf 'console line\n' >"$th/live/console-$n.txt"
  printf 'transcript\n' >"$runs/issue-$n/a-1-coder.json"
  mkdir -p "$works/widget-$n" "$works/widget-$n.state" "$works/widget-$n.images"
  printf 'the work\n' >"$works/widget-$n/README.md"
  : >"$works/widget-$n.before-retry"
  : >"$works/.tehas-workspaces"
}
# What this issue leaves that a stop should take, still on the host.
leftovers() { # <issue>
  local n=$1 out="" ext f
  for ext in "${GO[@]}"; do [ -e "$state/$n.$ext" ] && out="$out $state/$n.$ext"; done
  for f in "$th/state/park-$n.id" "$th/live/console-$n.txt" "$runs/issue-$n" \
    "$works/widget-$n" "$works/widget-$n.state" "$works/widget-$n.before-retry" "$works/widget-$n.images"; do
    [ -e "$f" ] && out="$out $f"
  done
  printf '%s\n' "${out# }"
}
# The record files of this issue that a stop keeps, still on the host.
record() { # <issue>
  local n=$1 out="" ext
  for ext in "${KEEP[@]}"; do [ -e "$state/$n.$ext" ] && out="$out $ext"; done
  printf '%s\n' "${out# }"
}
# The containers the stub was asked to remove.
removed_containers() { t_calls docker | sed -n 's/^rm -f //p' | tr '\n' ' '; }

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
t_eq "$(record 5)" "json history.jsonl by settings manifest.json approved.json rukas runner_exit status"
t_eq "$(removed_containers)" "tehas-work-5 "
t_contains "$t_out" "stopped tehas-cycle@5.service"
t_contains "$t_out" "removed the container tehas-work-5"
t_contains "$t_out" "removed state/cycles/5.lock"
t_contains "$t_out" "removed state/cycles/5.taken"
t_contains "$t_out" "removed state/park-5.id"
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
t_eq "$(wc -w <<<"$(leftovers 51)")" 22
t_eq "$(wc -w <<<"$(leftovers 55)")" 22
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
t_contains "$t_out" "removed state/cycles/5.park"
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
mkdir -p "$state"
( exec 9>"$state/5.lock"; flock -n 9 || exit 1; exec sleep 30 ) &
echo "$!" >"$T/holder-keep"
systemctl_live
T_ENV=(UNIT_STATE=active CONTAINERS=tehas-work-5)
began=$(date +%s)
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
kill "$(cat "$T/holder-keep" 2>/dev/null)" 2>/dev/null
t_eq "$(leftovers 5 | wc -w)" 22
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
hold 5
T_ENV=(CONTAINERS=)
t_run "$stop_cmd 5"
t_eq "$t_rc" 1
kill "$(cat "$T/holder" 2>/dev/null)" 2>/dev/null
t_eq "$(leftovers 5 | wc -w)" 22
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
rm -rf "$TEHAS_INSTANCE_DIR"
cycle_files 5
t_run "$stop_cmd 5"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$(leftovers 5 | wc -w)" 22
t_eq "$(t_calls systemctl)" ""
t_eq "$(t_calls docker)" ""
t_eq "$(t_calls gh)" ""

t_case "an issue that is not a number: 64, before anything is read"
reset
cycle_files 5
systemctl_live
docker_stub
for arg in 0 5a -5 12345678901 "" "''"; do
  t_run "$stop_cmd $arg"
  t_eq "$t_rc" 64
  t_eq "$(leftovers 5 | wc -w)" 22
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
t_eq "$(leftovers 5 | wc -w)" 22
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
t_eq "$(record 5)" "json history.jsonl by settings manifest.json approved.json rukas runner_exit status"

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
t_eq "$(record 5)" "json history.jsonl by settings manifest.json approved.json rukas runner_exit status"
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
t_eq "$(record 5)" "json history.jsonl by settings manifest.json approved.json rukas runner_exit status"
release

t_case "the read fits what gh really prints: gh's labels are an object, so a filter that indexes it as an array is a failure, not an empty answer"
reset
gh_labels tehas:running
t_run 'gh issue view 5 --repo acme/widget --json labels --jq ".labels[].name"'
t_eq "$t_out" "tehas:running"
t_run 'gh issue view 5 --repo acme/widget --json labels --jq ".[].name" >/dev/null 2>&1 || echo refused'
t_eq "$t_out" "refused"

t_done
