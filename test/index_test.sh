#!/usr/bin/env bash
# Tests for libexec/index: what the indexing container is given, which tree
# it reads, and that nothing is started when it should not be. A stub docker
# records its arguments, one per line, and answers as the indexer would.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

index="$T_ROOT/libexec/index"
checkout=$T/home/tehas/repos/widget
ws=$T/home/tehas/work/widget
marker=$T/home/tehas/state/index.json
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
# by the time the command returns.
stub='printf "%s\n" "$@" >"$T/calls/docker.args.$$"
[ "$1" = image ] && exit "${IMAGE_RC:-0}"
for a in "$@"; do
  case "$a" in *:ro) git -C "${a%%:*}" rev-parse HEAD >"$T/calls/indexed.head"; echo "${a%%:*}" >"$T/calls/indexed.dir" ;; esac
done
[ "${RUN_RC:-0}" = 0 ] || exit "$RUN_RC"
echo "${RUN_OUT:-{\"project\":\"p\",\"status\":\"indexed\",\"nodes\":5,\"edges\":7\}}"'
t_stub docker "$stub"

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
T_ENV=(IMAGE_RC=1)
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "sandbox/build.sh"
t_eq "$(runs)" 0
t_eq "$(find "$marker" 2>/dev/null | wc -l)" 0
T_ENV=()

t_case "while a cycle holds the dispatch lock nothing is indexed"
reset
mkdir -p "$T/home/tehas/state"
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle is running"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "the index is built in the sandbox image, at the path a cycle works in"
reset
T_ENV=(GH_TOKEN=test-token)
t_run "$index"
t_eq "$t_rc" 0
t_eq "$(run_args | sed -n "/^$(printf '%s' "$image" | sed 's/[\/.]/\\&/g')\$/,\$p" | tr '\n' ' ')" \
  "$image cli index_repository --repo-path $ws "
t_eq "$(after -w)" "$ws"
t_eq "$(after --entrypoint)" "codebase-memory-mcp"

t_case "the tree that is indexed is a clone at the branch's tip, read-only"
t_eq "$(cat "$T/calls/indexed.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(after -v | LC_ALL=C sort)" "$(printf '%s\n' "$(cat "$T/calls/indexed.dir"):$ws:ro" "pi-ensemble-cache:/home/vscode/.cache" | LC_ALL=C sort)"

t_case "the clone is gone afterwards and the cycle's workspace was never made"
t_eq "$(find "$(cat "$T/calls/indexed.dir")" 2>/dev/null | wc -l)" 0
t_eq "$(find "$T/home/tehas/work" 2>/dev/null | wc -l)" 0

t_case "the indexing container gets no network, no token and no option beyond these"
t_eq "$(after --network)" none
t_eq "$(after -e)" "HOME=/home/vscode"
t_eq "$(after --user)" "$(id -u):$(id -g)"
t_eq "$(run_args | unknown_options "$image" "--name --network --cpus --memory --user -e -w -v --entrypoint" "--rm")" ""
args=$(cat "$T"/calls/docker.args.*)
t_lacks "$args" "test-token"
t_lacks "$args" "GH_TOKEN"
t_lacks "$args" "docker.sock"
t_lacks "$args" ".ssh"
t_lacks "$args" ".config/tehas"
t_lacks "$args" "$checkout"
T_ENV=()

t_case "what was indexed is reported and recorded"
t_contains "$t_out" "5 nodes, 7 edges"
t_eq "$(jq -r .commit "$marker")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(jq -r .branch "$marker")" main
t_eq "$(jq -r '.nodes, .edges' "$marker" | tr '\n' ' ')" "5 7 "

t_case "the limits come from the settings"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_CPUS=1 TEHAS_WORK_MEMORY=512m
t_run "$index"
t_eq "$(after --cpus)" 1
t_eq "$(after --memory)" 512m
t_instance "${T_MINIMAL[@]}"

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

t_case "a failed indexer exits 1 and leaves the record as it was"
reset
before=$(cat "$marker")
T_ENV=(RUN_RC=1)
t_run "$index"
t_eq "$t_rc" 1
t_contains "$t_out" "$T/home/tehas/logs/index-"
t_eq "$(cat "$marker")" "$before"

t_case "an answer that does not say indexed counts as failed"
T_ENV=('RUN_OUT={"error":"no such path"}')
t_run "$index"
t_eq "$t_rc" 1
t_eq "$(cat "$marker")" "$before"
T_ENV=()

t_done
