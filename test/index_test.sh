#!/usr/bin/env bash
# Tests for libexec/index: what the indexing container is given, which tree
# it reads, and that nothing is started when it should not be. A stub docker
# records its arguments, one per line, and answers as the indexer would.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'
# The slice is the console's, which a checkout's libexec does not hold.
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE_CMD="$T_CONSOLE")

# The command as a host has it: beside tehas-cycle, which does the run.
# shellcheck disable=SC2154  # t_cycle_why is set by t_cycle (test/lib.sh)
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
mkdir -p "$T/tree/libexec"
ln -s "$T_ROOT/lib" "$T/tree/lib"
ln -s "$T_ROOT/libexec/index" "$T/tree/libexec/index"
cp "$T_CYCLE" "$T/tree/libexec/tehas-cycle"
index=$T/tree/libexec/index
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
marker=$T/home/tehas/state/index.json
failed=$T/home/tehas/state/index.failed
# Where the host keeps the index, and where the indexer has it.
indexdir=$T/home/tehas/state/index
cache=/home/vscode/.tehas-cache
image="tehas/pi-rukas:uid$(id -u)"
# The product repository, and the host's checkout of it.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
commit() { git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "$1"; }
commit first
git clone -q "$upstream" "$checkout"

# The two sandbox images: the id the stub answers with, and another one a case
# puts in its place with $IMAGE_ID.
image_id=sha256:1111111111111111111111111111111111111111111111111111111111111111
other_id=sha256:2222222222222222222222222222222222222222222222222222222222222222
# The stub answers `image inspect` with $IMAGE_ID and by $IMAGE_RC, and `run`
# by $RUN_RC. For a run it also notes the commit of the tree mounted read-only,
# which is gone by the time the command returns, and what the directory it is
# given read-write held when the run started. Only a run that succeeded writes a
# database into that directory, under the name of the tree, as the indexer
# would.
stub='if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
[ "$1" = image ] && { printf "%s\n" "${IMAGE_ID:-'"$image_id"'}"; exit "${IMAGE_RC:-0}"; }
[ "$1" = rm ] && exit 0
written=""
for a in "$@"; do
  case "$a" in
    *:ro) git -C "${a%%:*}" rev-parse HEAD >"$T/calls/indexed.head"; echo "${a%%:*}" >"$T/calls/indexed.dir" ;;
    *:/home/vscode/.tehas-cache) written=${a%%:*} ;;
  esac
done
[ -z "$written" ] || find "$written" -mindepth 1 -maxdepth 1 -printf "%f\n" | LC_ALL=C sort | tr "\n" " " >"$T/calls/index.at.run"
[ -z "${RUN_SLEEP:-}" ] || exec sleep "$RUN_SLEEP"
[ "${RUN_RC:-0}" = 0 ] || exit "$RUN_RC"
echo index >"$written/widget.db"
echo "${RUN_OUT:-{\"project\":\"p\",\"status\":\"indexed\",\"nodes\":5,\"edges\":7\}}"'
t_stub docker "$stub"
# The real timeout. Around the indexer, the one called with -k, the bound is
# $TIMEOUT_BOUND when it is set.
t_stub timeout 'real=$(PATH=${PATH#"$T/bin:"} command -v timeout)
if [ -n "${TIMEOUT_BOUND:-}" ] && [ "$1" = -k ]; then exec "$real" -k 1 "$TIMEOUT_BOUND" "${@:4}"; fi
exec "$real" "$@"'
# A host with the root step done: the slice loaded from its file with 8 GiB,
# so a container's ceiling is 2048m, Docker on the systemd driver and cgroup
# v2, and nothing in the slice.
t_stub systemctl 'cat "$T/slice.show"'
slice_good() {
  printf '%s\n' FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592 >"$T/slice.show"
  echo "systemd 2" >"$T/docker.info"
  slice_state 0.00
}
slice_good
limits="--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares"
root="TEHAS_CGROUP_ROOT=$T_CGROUP"
# A run waits this long for a poll that holds the dispatcher's lock.
wait="TEHAS_INDEX_LOCK_WAIT=1"

run_args() { grep -l -x -e index_repository "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { run_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
# Every option that is not one this command is known to pass, up to the image
# name, read the way docker reads them. See test/work_lib.sh.
unknown_options() { # <image> <options that take a value> <options that take none>
  awk -v image="$1" -v valued=" $2 " -v plain=" $3 " '
    NR == 1 { next } # "run"
    skip { skip = 0; next }
    $0 == image { exit }
    index(valued, " " $0 " ") { skip = 1; next }
    index(plain, " " $0 " ") { next }
    { print }'
}
runs() { grep -l -x -e index_repository "$T"/calls/docker.args.* 2>/dev/null | wc -l; }
reset() { rm -f "$T"/calls/*; }

t_case "an invalid instance starts no container"
T_ENV=("$root" "$wait")
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$index"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""

t_case "an argument is a usage error"
t_instance "${T_MINIMAL[@]}"
for bad in now "--before-cycle now" --force; do
  t_run "$index $bad"
  t_eq "$t_rc" 64
  t_eq "$(t_calls docker)" ""
done

t_case "a checkout of another repository starts no container"
git -C "$checkout" remote set-url origin https://github.com/acme/other.git
t_run "$index"
t_eq "$t_rc" 78
t_contains "$t_out" "not of acme/widget"
t_eq "$(t_calls docker)" ""
git -C "$checkout" remote set-url origin "$upstream"

t_case "without the sandbox image nothing is indexed and the build is named"
T_ENV=(IMAGE_RC=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox/build.sh"
t_eq "$(runs)" 0
t_eq "$(find "$marker" 2>/dev/null | wc -l)" 0
T_ENV=("$root" "$wait")

t_case "while a poll holds the dispatcher's lock for all of the wait nothing is indexed"
reset
mkdir -p "$T/home/tehas/state"
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
t_contains "$t_out" "a poll held the dispatcher's lock for 1s, nothing was indexed"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "a run waits for a poll that lets the dispatcher's lock go, and lets it go itself"
reset
mkdir -p "$T/home/tehas/state"
( exec 9>"$T/home/tehas/state/dispatch.lock"; flock -n 9; sleep 0.5 ) &
sleep 0.2
T_ENV=("$root" TEHAS_INDEX_LOCK_WAIT=5)
t_run "$index"
T_ENV=("$root" "$wait")
wait
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_eq "$(flock -n "$T/home/tehas/state/dispatch.lock" echo free)" free

t_case "while another index run holds its lock nothing is indexed"
reset
exec 9>"$T/home/tehas/state/index.lock"
flock -n 9
t_run "$index --before-cycle"
t_eq "$t_rc" 3
t_contains "$t_out" "another index run holds its lock"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "before a cycle, a cycle that runs is not indexed under"
reset
commit cycle
git -C "$checkout" fetch -q
mkdir -p "$T/home/tehas/state/cycles"
exec 9>"$T/home/tehas/state/cycles/12.lock"
flock -n 9
t_run "$index --before-cycle"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle is running (issue 12)"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "while a cycle holds its lock nothing is indexed, and the issue is named"
reset
mkdir -p "$T/home/tehas/state/cycles"
mkdir -p "$T/home/tehas/state/cycles"
exec 9>"$T/home/tehas/state/cycles/12.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle is running (issue 12)"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "the lock file of a cycle that ended holds nothing back"
reset
t_run "$index"
t_eq "$t_rc" 0
rm -rf "$T/home/tehas/state/cycles"

t_case "the index is built in the sandbox image, at the path a cycle works in"
reset
T_ENV=(GH_TOKEN=test-token "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(run_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" \
  "$image cli index_repository --repo-path $ws "
t_eq "$(after -w)" "$ws"
t_eq "$(after --entrypoint)" "codebase-memory-mcp"

t_case "the tree that is indexed is a clone at the branch's tip, read-only"
t_eq "$(cat "$T/calls/indexed.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$(cat "$T/calls/indexed.dir"):$ws:ro" "$indexdir:$cache" | LC_ALL=C sort)"

t_case "the indexer writes the host's own index directory, and nothing else"
t_eq "$(after -v | grep -c -v ':ro$')" 1
t_eq "$(after -e | LC_ALL=C sort | tr '\n' ' ')" "CBM_CACHE_DIR=$cache HOME=/home/vscode "

t_case "the clone is gone afterwards and the cycle's workspace was never made"
t_eq "$(find "$(cat "$T/calls/indexed.dir")" 2>/dev/null | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" 2>/dev/null | wc -l)" 0

t_case "the indexing container gets no network, no token and no option beyond these"
t_eq "$(after --network)" none
t_eq "$(after --user)" "$(id -u):$(id -g)"
# An init collects what git and the indexer leave behind, as in a cycle.
t_eq "$(run_args | grep -c -x -e --init)" 1
t_eq "$(run_args | unknown_options "$image" "--name --network $limits --user -e -w -v --entrypoint" "--rm --init")" ""
args=$(cat "$T"/calls/docker.args.*)
t_lacks "$args" "test-token"
t_lacks "$args" "GH_TOKEN"
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "$checkout"
T_ENV=("$root" "$wait")

t_case "what was indexed is reported and recorded"
t_contains "$t_out" "5 nodes, 7 edges"
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(jq -r .branch "$marker")" main
t_eq "$(jq -r '.nodes, .edges' "$marker" | tr '\n' ' ')" "5 7 "

t_case "the indexer gets the slice and a cycle's limits"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=1g
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(after --cgroup-parent)" "tehas.slice"
t_eq "$(after --memory)" "2048m"
t_eq "$(after --memory-swap)" "2048m"
t_eq "$(after --memory-reservation)" "1g"
t_eq "$(after --pids-limit)" "4096"
t_eq "$(after --cpu-shares)" "1024"
t_eq "$(after --network)" none
t_eq "$(after --cpus)" "15.00"
t_lacks "$(run_args)" "--cidfile"
t_eq "$(run_args | unknown_options "$image" "--name --network $limits --user -e -w -v --entrypoint" "--rm --init")" ""
t_instance "${T_MINIMAL[@]}"

t_case "the slice is read once: one systemctl call, one docker info"
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax"
t_eq "$(t_calls docker | grep -c '^info ')" 1

# --- Without a usable slice ----------------------------------------------------
# refused: 78, no call to docker but the question about the slice, no clone,
# and the record as it was.
kept=$(cat "$marker")
refused() {
  t_eq "$t_rc" 78
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_eq "$(t_calls docker | grep -c -v '^info ')" 0
  t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
  t_eq "$(cat "$marker")" "$kept"
}
without() { reset; slice_good; }

t_case "refuses with 78 without the slice, before any docker run"
without
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$index"
refused
t_contains "$t_out" "tehas index: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  without
  echo "$answer" >"$T/docker.info"
  t_run "$index"
  refused
  t_contains "$t_out" "tehas index: "
  t_contains "$t_out" "ocker"
done

t_case "a reserve above the ceiling is refused"
without
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$index"
refused
t_contains "$t_out" "tehas index: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m"
t_instance "${T_MINIMAL[@]}"

t_case "before a cycle, without the slice nothing is indexed either"
without
commit unseen
git -C "$checkout" fetch -q
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$index --before-cycle"
refused
without
t_run "$index"
t_eq "$t_rc" 0

t_case "it indexes what the checkout last fetched, not the checkout's own tree"
reset
commit second
git -C "$checkout" fetch -q
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/indexed.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
[ "$(git -C "$checkout" rev-parse HEAD)" != "$(git -C "$upstream" rev-parse main)" ] || t_bad "the checkout's own tree moved"

t_case "before a cycle, an index already at the branch's tip is left alone"
reset
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_contains "$t_out" "already indexed"
t_eq "$(t_calls docker)" ""

t_case "before a cycle, a branch that moved is indexed"
commit third
git -C "$checkout" fetch -q
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
exec 9>&-

t_case "before a cycle, an empty index directory is built again though the record names the tip"
reset
rm -rf "$indexdir"
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_lacks "$t_out" "already indexed"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 | wc -l)" 1

t_case "before a cycle, an index directory with only the indexer's logs is built again"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_lacks "$t_out" "already indexed"

t_case "before a cycle, leftovers are no index: the config and a staging file are built over"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo cfg >"$indexdir/_config.db"
echo left >"$indexdir/widget.db.stage.abc123.tmp.945328.0x200002bc000"
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_lacks "$t_out" "already indexed"

t_case "a run removes what an earlier one left in the index directory"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo old >"$indexdir/other-tree.db"
echo left >"$indexdir/widget.db.stage.abc123.tmp.945328.0x200002bc000"
echo gone >"$indexdir/scratch.tmp"
touch -d "@$(( $(date +%s) - 120 ))" "$indexdir/other-tree.db"
echo log >"$indexdir/logs/index-1.log"
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "logs widget.db "
t_eq "$(cat "$indexdir/widget.db")" index
t_contains "$t_out" "removed 3 file(s) an earlier run left in $indexdir"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -type d -printf '%f\n')" "logs"

t_case "a database written by this run is never pruned, nor the indexer's own logs"
reset
rm -rf "$indexdir"
# The same name as the one this run writes: pruned only when it predates the run.
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_lacks "$t_out" "removed "
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "widget.db "
commit prune
git -C "$checkout" fetch -q
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_lacks "$t_out" "removed "
t_eq "$(cat "$indexdir/widget.db")" index

t_case "a failed run prunes nothing and leaves the leftovers where they were"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo old >"$indexdir/other-tree.db"
echo left >"$indexdir/widget.db.stage.abc123.tmp.945328.0x200002bc000"
T_ENV=(RUN_RC=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_lacks "$t_out" "removed "
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "logs other-tree.db widget.db.stage.abc123.tmp.945328.0x200002bc000 "
T_ENV=("$root" "$wait")

t_case "a failed indexer exits 1, leaves the record as it was and notes the tip"
reset
before=$(cat "$marker")
tip=$(git -C "$upstream" rev-parse main)
T_ENV=(RUN_RC=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "$T/home/tehas/logs/index-"
t_eq "$(cat "$marker")" "$before"
t_eq "$(cat "$failed")" "$tip"

t_case "an answer that does not say indexed counts as failed"
rm -f "$failed"
T_ENV=('RUN_OUT={"error":"no such path"}' "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_eq "$(cat "$marker")" "$before"
t_eq "$(cat "$failed")" "$tip"
T_ENV=("$root" "$wait")

t_case "a container left by a killed run is removed before the run, under timeout"
reset
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -v '^info ' | cut -d' ' -f1-3 | tr '\n' ' ')" "image inspect $image rm -f tehas-index run --rm --init "
t_eq "$(t_calls timeout | grep '^-k ' | cut -d' ' -f1-5)" "-k 30 1800 docker run"
t_eq "$(find "$failed" 2>/dev/null | wc -l)" 0

t_case "an indexer that hangs is stopped at the bound, and its container removed"
reset
began=$(date +%s)
T_ENV=(RUN_SLEEP=60 TIMEOUT_BOUND=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
[ "$(($(date +%s) - began))" -lt 20 ] || t_bad "the run was not stopped at the bound"
t_contains "$t_out" "the indexer did not finish within"
t_eq "$(t_calls docker | grep -c '^rm -f tehas-index$')" 2
T_ENV=("$root" "$wait")

t_case "before a cycle, two failures at one tip give one run"
commit fails
git -C "$checkout" fetch -q
tip=$(git -C "$upstream" rev-parse main)
before=$(jq -r .commit "$marker")
reset
T_ENV=(RUN_RC=1 "$root" "$wait")
t_run "$index --before-cycle"
t_eq "$t_rc" 1
t_run "$index --before-cycle"
t_eq "$t_rc" 1
t_contains "$t_out" "not tried again before the branch moves"
t_eq "$(runs)" 1
t_eq "$(jq -r .commit "$marker")" "$before"
t_eq "$(cat "$failed")" "$tip"

t_case "by hand a failed tip is tried again, and a branch that moved is tried before a cycle"
reset
t_run "$index"
t_eq "$t_rc" 1
t_eq "$(runs)" 1
T_ENV=("$root" "$wait")
commit mended
git -C "$checkout" fetch -q
reset
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(find "$failed" 2>/dev/null | wc -l)" 0

# --- The sandbox image the index was built with -------------------------------
# The record names the image that ran. An index written by another image, or by
# one that recorded nothing, is removed and built from nothing; the same image
# leaves what it wrote for the indexer to update. What the directory given
# read-write held when the run started ($T/calls/index.at.run) tells them apart.
t_case "the record names the sandbox image that built the index"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo old >"$indexdir/other-tree.db"
commit recorded
git -C "$checkout" fetch -q
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(jq -r .indexer_image "$marker")" "$image_id"
t_lacks "$t_out" "from scratch"
t_eq "$(cat "$T/calls/index.at.run")" "logs other-tree.db "

t_case "an index built by another sandbox image is built from scratch"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo cfg >"$indexdir/_config.db"
echo old >"$indexdir/other-tree.db"
echo left >"$indexdir/widget.db.stage.abc123.tmp.945328.0x200002bc000"
T_ENV=(IMAGE_ID="$other_id" "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 0
t_contains "$t_out" "the index was built by sandbox image $image_id, this run uses $other_id: 2 file(s) it wrote are set aside and the index is built from scratch"
t_eq "$(cat "$T/calls/index.at.run")" "_config.db logs "
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "_config.db logs widget.db "
t_eq "$(jq -r .indexer_image "$marker")" "$other_id"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0

t_case "the same sandbox image updates the index it built"
reset
echo keep >"$indexdir/other-tree.db"
commit updated
git -C "$checkout" fetch -q
t_run "$index"
t_eq "$t_rc" 0
t_lacks "$t_out" "sandbox image"
t_eq "$(cat "$T/calls/index.at.run")" "_config.db logs other-tree.db widget.db "
t_eq "$(jq -r .indexer_image "$marker")" "$other_id"
T_ENV=("$root" "$wait")

t_case "before a cycle, a moved branch is built from scratch when the image changed"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir"
echo old >"$indexdir/widget.db"
T_ENV=(IMAGE_ID="$image_id" "$root" "$wait")
commit moved
git -C "$checkout" fetch -q
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_contains "$t_out" "the index was built by sandbox image $other_id, this run uses $image_id"
t_eq "$(cat "$T/calls/index.at.run")" ""
t_eq "$(jq -r .indexer_image "$marker")" "$image_id"

t_case "an index with no recorded image is built from scratch"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir"
echo old >"$indexdir/widget.db"
jq 'del(.indexer_image)' "$marker" >"$marker.new" && mv "$marker.new" "$marker"
commit unrecorded
git -C "$checkout" fetch -q
t_run "$index"
t_eq "$t_rc" 0
t_contains "$t_out" "the index was built by sandbox image unknown, this run uses $image_id"
t_eq "$(cat "$T/calls/index.at.run")" ""
t_eq "$(jq -r .indexer_image "$marker")" "$image_id"

t_case "a host that has never been indexed says nothing about images"
reset
rm -rf "$indexdir"
rm -f "$marker"
commit fresh
git -C "$checkout" fetch -q
t_run "$index"
t_eq "$t_rc" 0
t_lacks "$t_out" "sandbox image"
t_eq "$(cat "$T/calls/index.at.run")" ""
t_eq "$(jq -r .indexer_image "$marker")" "$image_id"

t_case "what the indexer wrote for itself is not what a rebuild removes"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo cfg >"$indexdir/_config.db"
T_ENV=(IMAGE_ID="$other_id" "$root" "$wait")
commit nothing
git -C "$checkout" fetch -q
t_run "$index"
t_eq "$t_rc" 0
t_lacks "$t_out" "sandbox image"
t_eq "$(cat "$T/calls/index.at.run")" "_config.db logs "
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "_config.db logs widget.db "
T_ENV=("$root" "$wait")

# The record now names $other_id and the stub answers with $image_id, so this
# run rebuilds. It is the dispatcher's own case: the rebuild is what the run
# that fails has to put back, because the cycles get whatever is in the index
# directory until a run succeeds.
t_case "a rebuild that fails before a cycle leaves the cycles the index they had"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo cfg >"$indexdir/_config.db"
echo old >"$indexdir/other-tree.db"
kept=$(cat "$marker")
commit broken
git -C "$checkout" fetch -q
T_ENV=(IMAGE_ID="$image_id" RUN_RC=1 "$root" "$wait")
t_run "$index --before-cycle"
t_eq "$t_rc" 1
t_contains "$t_out" "the index was built by sandbox image $other_id, this run uses $image_id: 1 file(s) it wrote are set aside and the index is built from scratch"
t_eq "$(cat "$T/calls/index.at.run")" "_config.db logs "
t_eq "$(cat "$failed")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(cat "$marker")" "$kept"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "_config.db logs other-tree.db "
t_eq "$(cat "$indexdir/other-tree.db")" old
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
t_run "$index --before-cycle"
t_eq "$t_rc" 1
t_contains "$t_out" "not tried again before the branch moves"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "_config.db logs other-tree.db "
T_ENV=("$root" "$wait")

t_case "a rebuild that fails by hand puts the index back where the next run finds it"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo cfg >"$indexdir/_config.db"
echo old >"$indexdir/other-tree.db"
T_ENV=(IMAGE_ID="$image_id" RUN_RC=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "1 file(s) it wrote are set aside"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "_config.db logs other-tree.db "
T_ENV=(IMAGE_ID="$image_id" "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/index.at.run")" "_config.db logs "
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "_config.db logs widget.db "
t_eq "$(jq -r .indexer_image "$marker")" "$image_id"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
T_ENV=("$root" "$wait")

# The set-aside goes back only for a run that wrote no index. An indexer that
# wrote its database and then answered in a shape this command does not accept
# is the case: what it wrote is the index the host keeps, and putting the other
# image's files back over it would lose it and mix two images' data.
t_case "a rebuild whose indexer wrote a database keeps it and drops what it set aside"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo cfg >"$indexdir/_config.db"
echo old >"$indexdir/other-tree.db"
jq --arg i "$other_id" '.indexer_image = $i' "$marker" >"$marker.new" && mv "$marker.new" "$marker"
kept=$(cat "$marker")
tip=$(git -C "$upstream" rev-parse main)
T_ENV=('RUN_OUT={"error":"no such path"}' "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "the index was built by sandbox image $other_id, this run uses $image_id: 1 file(s) it wrote are set aside and the index is built from scratch"
t_contains "$t_out" "this run wrote index data, what the other sandbox image wrote was dropped"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "_config.db logs widget.db "
t_eq "$(cat "$indexdir/widget.db")" index
t_eq "$(cat "$failed")" "$tip"
t_eq "$(cat "$marker")" "$kept"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
# The database the rejected run wrote is what the cycles are given, and before
# a cycle that is what the record says is the branch's tip, so nothing runs.
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_contains "$t_out" "already indexed"
t_eq "$(cat "$indexdir/widget.db")" index
T_ENV=("$root" "$wait")

# A run that was killed never ran its trap, so what it set aside waits in
# $TEHAS_HOME/tmp and the index directory holds nothing a cycle can search.
t_case "what a killed run left set aside goes back where the next run can use it"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs" "$T/home/tehas/tmp"
orphan=$(mktemp -d "$T/home/tehas/tmp/index.stale.XXXXXX")
echo old >"$orphan/other-tree.db"
kept=$(cat "$marker")
T_ENV=(RUN_RC=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "a run that was killed left index data set aside: 1 file(s) put back in $indexdir, 0 removed"
t_contains "$t_out" "1 file(s) it wrote are set aside"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "logs other-tree.db "
t_eq "$(cat "$indexdir/other-tree.db")" old
t_eq "$(cat "$marker")" "$kept"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
T_ENV=("$root" "$wait")

t_case "what a killed run left set aside is removed when the index holds a database"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs" "$T/home/tehas/tmp"
echo keep >"$indexdir/widget.db"
touch -d "@$(( $(date +%s) - 120 ))" "$indexdir/widget.db"
orphan=$(mktemp -d "$T/home/tehas/tmp/index.stale.XXXXXX")
echo old >"$orphan/other-tree.db"
T_ENV=(RUN_RC=1 "$root" "$wait")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "a run that was killed left index data set aside: 0 file(s) put back in $indexdir, 1 removed"
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "logs widget.db "
t_eq "$(cat "$indexdir/widget.db")" keep
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
T_ENV=("$root" "$wait")

t_done
