#!/usr/bin/env bash
# Tests for libexec/index: what the indexing container is given, which tree
# it reads, and that nothing is started when it should not be. A stub docker
# records its arguments, one per line, and answers as the indexer would.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
# The CPU quota and the ticket ceiling follow the host's CPUs: the same on
# every test machine.
t_stub nproc 'echo 16'

index="$T_ROOT/libexec/index"
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

# The stub answers `image inspect` by $IMAGE_RC and `run` by $RUN_RC. For a
# run it also notes the commit of the tree mounted read-only, which is gone
# by the time the command returns. Only a run that succeeded writes a database
# into the directory it is mounted read-write, under the name of the tree, as
# the indexer would.
stub='if [ "$1" = info ]; then cat "$T/docker.info"; exit; fi
printf "%s\n" "$@" >"$T/calls/docker.args.$$"
[ "$1" = image ] && exit "${IMAGE_RC:-0}"
[ "$1" = rm ] && exit 0
written=""
for a in "$@"; do
  case "$a" in
    *:ro) git -C "${a%%:*}" rev-parse HEAD >"$T/calls/indexed.head"; echo "${a%%:*}" >"$T/calls/indexed.dir" ;;
    *:/home/vscode/.tehas-cache) written=${a%%:*} ;;
  esac
done
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

run_args() { grep -l -x -e index_repository "$T"/calls/docker.args.* 2>/dev/null | head -n 1 | xargs -r cat; }
after() { run_args | awk -v flag="$1" 'prev == flag { print } { prev = $0 }'; }
# Every option that is not one this command is known to pass, up to the image
# name, read the way docker reads them. See test/work_test.sh.
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
T_ENV=("$root")
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
T_ENV=(IMAGE_RC=1 "$root")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox/build.sh"
t_eq "$(runs)" 0
t_eq "$(find "$marker" 2>/dev/null | wc -l)" 0
T_ENV=("$root")

t_case "while the dispatcher's lock is held nothing is indexed"
reset
mkdir -p "$T/home/tehas/state"
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
t_contains "$t_out" "the dispatcher's lock is held"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "while a cycle holds its lock nothing is indexed, and the issue is named"
reset
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
T_ENV=(GH_TOKEN=test-token "$root")
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
T_ENV=("$root")

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
t_eq "$(after --cpus)" "3.75"
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

t_case "the fourteen words are all present or nothing starts"
# The command beside a slice library that says yes and hands out a short list,
# or a list that names another parent.
short=$T/short
mkdir -p "$short/libexec" "$short/lib"
cp "$T_ROOT"/lib/*.sh "$short/lib/"
ln -s "$index" "$short/libexec/index"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" ""; do
  without
  printf '%s\n' 'tehas_slice_ok() { return 0; }' "tehas_slice_limits() { TEHAS_SLICE_LIMITS=($words); }" >"$short/lib/slice.sh"
  t_run "$short/libexec/index"
  refused
  t_contains "$t_out" "tehas index: the container limits could not be made, not starting"
done

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

t_case "before a cycle, a branch that moved is indexed while the caller holds the lock"
commit third
git -C "$checkout" fetch -q
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
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
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index --before-cycle"
exec 9>&-
t_eq "$t_rc" 0
t_lacks "$t_out" "removed "
t_eq "$(cat "$indexdir/widget.db")" index

t_case "a failed run prunes nothing and leaves the leftovers where they were"
reset
rm -rf "$indexdir"
mkdir -p "$indexdir/logs"
echo old >"$indexdir/other-tree.db"
echo left >"$indexdir/widget.db.stage.abc123.tmp.945328.0x200002bc000"
T_ENV=(RUN_RC=1 "$root")
t_run "$index"
t_eq "$t_rc" 1
t_lacks "$t_out" "removed "
t_eq "$(find "$indexdir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "logs other-tree.db widget.db.stage.abc123.tmp.945328.0x200002bc000 "
T_ENV=("$root")

t_case "a failed indexer exits 1, leaves the record as it was and notes the tip"
reset
before=$(cat "$marker")
tip=$(git -C "$upstream" rev-parse main)
T_ENV=(RUN_RC=1 "$root")
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "$T/home/tehas/logs/index-"
t_eq "$(cat "$marker")" "$before"
t_eq "$(cat "$failed")" "$tip"

t_case "an answer that does not say indexed counts as failed"
rm -f "$failed"
T_ENV=('RUN_OUT={"error":"no such path"}' "$root")
t_run "$index"
t_eq "$t_rc" 1
t_eq "$(cat "$marker")" "$before"
t_eq "$(cat "$failed")" "$tip"
T_ENV=("$root")

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
T_ENV=(RUN_SLEEP=60 TIMEOUT_BOUND=1 "$root")
t_run "$index"
t_eq "$t_rc" 1
[ "$(($(date +%s) - began))" -lt 20 ] || t_bad "the run was not stopped at the bound"
t_contains "$t_out" "the indexer did not finish within"
t_eq "$(t_calls docker | grep -c '^rm -f tehas-index$')" 2
T_ENV=("$root")

t_case "before a cycle, two failures at one tip give one run"
commit fails
git -C "$checkout" fetch -q
tip=$(git -C "$upstream" rev-parse main)
before=$(jq -r .commit "$marker")
reset
T_ENV=(RUN_RC=1 "$root")
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
T_ENV=("$root")
commit mended
git -C "$checkout" fetch -q
reset
t_run "$index --before-cycle"
t_eq "$t_rc" 0
t_eq "$(runs)" 1
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(find "$failed" 2>/dev/null | wc -l)" 0

t_done
