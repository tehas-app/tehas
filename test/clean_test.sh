#!/usr/bin/env bash
# Tests for libexec/clean: what it removes in a kept workspace, what it never
# touches, the age rule, the lock, the dry run, the settings, the one docker
# call, and what a failing docker leaves running.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

clean="$T_ROOT/libexec/clean"
fx=$T/fx
th=$T/home/tehas
workdir=$th/work/widget
cycles=$th/state/cycles
logs=$th/logs
tmp=$th/tmp
logfile=$logs/clean.log
mkdir -p "$fx"

# The host's build cache is docker's to prune; the stub answers with one line
# and records the call. DOCKER_RC makes it fail.
t_stub docker 'case "$1 $2" in
  "builder prune") [ "${DOCKER_RC:-0}" = 0 ] || { echo "docker: cannot connect" >&2; exit "${DOCKER_RC:-1}"; }
    echo "Total: 4.2GB" ;;
  *) exit 1 ;;
esac'

reset() {
  rm -rf "$T"/calls/* "$th"
  mkdir -p "$cycles" "$logs" "$tmp/dl" "$tmp/node-compile-cache" "$tmp/improve" "$workdir"
  t_instance "${T_MINIMAL[@]}"
}

# An issue's kept workspace and the two directories beside it, all three days
# old: what a cycle left, plus what a person would still want to read.
# 7 is the issue; <issue> defaults to 7 and the suffix to nothing. A case that
# wants one part of the tree young says which with the last argument: the rule
# reads the age of the workspace, its state, its before-retry and the issue's
# lock, so those are the paths worth naming.
workspace() { # [issue] [suffix] [young paths...]
  local n=${1:-7} suffix=${2:-}
  shift 2
  local d=$workdir-$n$suffix s=$workdir-$n.state p
  mkdir -p "$d/target/sub" "$d/node_modules/pkg" "$d/src" "$d/sub/target" \
    "$d/nested/target" "$d/.git" "$d/keep"
  echo keep >"$d/src/main.sh"
  echo keep >"$d/uncommitted"
  echo obj >"$d/.git/HEAD"
  echo keep >"$d/keep/notes"
  echo big >"$d/target/sub/app"
  echo big >"$d/node_modules/pkg/index.js"
  echo big >"$d/sub/target/app"
  echo big >"$d/nested/target/app"
  mkdir -p "$s/cache" "$s/memory" "$s/sessions"
  echo idx >"$s/cache/index.db"
  echo m >"$s/memory/learned"
  echo p >"$s/sessions/one.jsonl"
  age "$d" "$d/target" "$d/node_modules" "$d/sub" "$d/sub/target" "$d/nested" \
    "$d/nested/target" "$s" "$s/cache" "$cycles/$n.lock"
  for p in "$@"; do touch "$p"; done
}
age() { touch -d '3 days ago' "$@"; }

t_case "an idle kept workspace loses its build output and keeps the rest"
reset
workspace
t_run "$clean"
t_eq "$t_rc" 0
for gone in target node_modules sub/target nested/target; do
  t_eq "$([ -e "$workdir-7/$gone" ] && echo there)" ""
done
for kept in src/main.sh uncommitted .git/HEAD keep/notes; do
  t_eq "$([ -e "$workdir-7/$kept" ] && echo there)" "there"
done
for kept in memory/learned sessions/one.jsonl; do
  t_eq "$([ -e "$workdir-7.state/$kept" ] && echo there)" "there"
done
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" ""
t_contains "$t_out" "removed $workdir-7/target: build output of #7"
t_contains "$t_out" "removed $workdir-7/node_modules: build output of #7"
t_contains "$t_out" "removed $workdir-7/nested/target: build output of #7"
t_contains "$t_out" "removed $workdir-7.state/cache: the code index copy of #7"
t_contains "$t_out" "freed"
t_eq "$(printf '%s\n' "$t_out" | grep -c 'clean:')" "$(wc -l <"$logfile")"

t_case "build output under a nested tree of the workspace goes too"
reset
workspace
mkdir -p "$workdir-7/pkg/ext/target"
echo x >"$workdir-7/pkg/ext/target/app"
age "$workdir-7/pkg" "$workdir-7/pkg/ext" "$workdir-7/pkg/ext/target"
# A new name in the workspace is a new entry in its directory: the age rule
# reads the workspace itself, so a case that adds to it ages that too.
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/pkg/ext/target" ] && echo there)" ""

t_case "a workspace touched within the age is left alone"
reset
workspace 7 "" "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/node_modules" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_lacks "$t_out" "removed $workdir-7/target"

t_case "a workspace whose issue has a running cycle is left alone"
reset
workspace
flock -n "$cycles/7.lock" sleep 5 &
sleep 0.3
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
wait

t_case "a workspace whose lock another holds is left alone"
reset
# A lock file that exists and is held, with no cycle behind it: the age rule
# passes and the lock is what keeps the build output. The lock is taken before
# the fixture is made, so the fixture must not clear the home.
mkdir -p "$cycles" "$logs" "$tmp" "$workdir"
# This shell's own flock does not stop a second flock of the same file from a
# different process, so the holder is a sleep of its own. The fixture then makes
# issue 8 and ages it, all of it while that sleep holds the lock.
: >"$cycles/8.lock"
flock "$cycles/8.lock" sleep 10 &
sleep 0.3
flock -n "$cycles/8.lock" true && t_bad 'the holder did not take the lock'
mkdir -p "$workdir-8/target" "$workdir-8/src" "$workdir-8.state/cache"
echo x >"$workdir-8/target/app"
echo idx >"$workdir-8.state/cache/index.db"
age "$cycles/8.lock"
age "$workdir-8" "$workdir-8/target" "$workdir-8.state" "$workdir-8.state/cache" "$cycles/8.lock"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-8/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-8.state/cache" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #8: its lock is held"
wait

t_case "a symlink named like build output is neither followed nor removed"
reset
workspace
mkdir -p "$T/outside"
echo stay >"$T/outside/file"
ln -s "$T/outside" "$workdir-7/target-link"
ln -s "$T/outside" "$workdir-7/node_modules" 2>/dev/null || true
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/outside/file" ] && echo there)" "there"
t_eq "$([ -L "$workdir-7/target-link" ] && echo link)" "link"

t_case "the host scratch of that age goes, and nothing else under tmp"
reset
echo old >"$tmp/dl/tool.tar.gz"
echo old >"$tmp/node-compile-cache/blob"
echo keep >"$tmp/improve/record.json"
mkdir -p "$tmp/tehas-test.OLD"
echo old >"$tmp/tehas-test.OLD/x"
age "$tmp/dl/tool.tar.gz" "$tmp/node-compile-cache/blob" "$tmp/improve/record.json" \
  "$tmp/tehas-test.OLD" "$tmp/tehas-test.OLD/x"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""
t_eq "$([ -e "$tmp/node-compile-cache/blob" ] && echo there)" ""
t_eq "$([ -e "$tmp/tehas-test.OLD" ] && echo there)" ""
t_eq "$([ -e "$tmp/improve/record.json" ] && echo there)" "there"

t_case "young host scratch stays"
reset
echo new >"$tmp/dl/tool.tar.gz"
echo old >"$tmp/dl/old.tar.gz"
age "$tmp/dl/old.tar.gz"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" "there"
t_eq "$([ -e "$tmp/dl/old.tar.gz" ] && echo there)" ""

t_case "--dry-run removes nothing and logs what it would remove"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
t_run "$clean --dry-run"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/node_modules" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" "there"
t_contains "$t_out" "would remove $workdir-7/target: build output of #7"
t_contains "$t_out" "would remove $tmp/dl/tool.tar.gz: host scratch"
t_contains "$t_out" "freed 0 bytes"
t_lacks "$t_out" "clean: removed"

t_case "the docker step is asked for the cache of the instance's age"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=6
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$(t_calls docker)" "builder prune -f --filter until=6h"

t_case "a failing docker is a warning, the other steps still run, and the exit is 1"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
T_ENV=(DOCKER_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker builder prune failed"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""

t_case "a prune that does not finish in time is a warning, and the scratch step still runs"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
# A docker that answers as timeout(1) does when it ran out.
T_ENV=(DOCKER_RC=124)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker builder prune did not finish in 600 seconds"
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""

t_case "a bad TEHAS_CLEAN_AFTER stops the run before any docker call"
reset
workspace
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=soon >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$clean"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"

t_case "a TEHAS_CLEAN_DIRS of .git stops the run before any docker call"
reset
workspace
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_CLEAN_DIRS='.git target'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$clean"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/.git/HEAD" ] && echo there)" "there"

t_case "an unknown argument is 64 and changes nothing"
reset
workspace
t_run "$clean --yes"
t_eq "$t_rc" 64
t_eq "$(t_calls docker)" ""
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"

t_case "the help text says what it removes, its usage and its exits"
t_run "$clean --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot clean"
t_contains "$t_out" "Usage:"
t_contains "$t_out" "Exit:"
t_lacks "$t_out" "unknown argument"

t_case "a second run while one holds the lock does nothing and says so"
reset
mkdir -p "$cycles" "$logs" "$tmp" "$workdir"
exec 6>>"$th/state/clean.lock"
flock 6
workspace
t_run "$clean"
t_eq "$t_rc" 3
t_contains "$t_out" "another run holds the lock"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
exec 6>&-

t_case "an instance with no settings at all is refused before anything runs"
rm -rf "$TEHAS_INSTANCE_DIR"
t_run "$clean"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""

t_done
