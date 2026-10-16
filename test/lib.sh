# shellcheck shell=bash
# shellcheck disable=SC2034  # the variables here are read by the test files
# Helpers for the shell tests. Source this from a *_test.sh file.
#
#   t_case "what is being checked"   start a case; later failures name it
#   t_run '<bash snippet>'           run it in a clean environment; sets t_out, t_rc
#   t_eq / t_contains / t_lacks      assertions, they record a failure and go on
#   t_skip <reason>                  say a case ran only in part, and count it
#   t_instance KEY=VALUE...          write a scratch instance directory
#   t_stub NAME ['<bash body>']      a stub command that records its arguments
#   t_calls NAME                     what the stub was called with, one call per line
#   t_stub_cargo                     a cargo that compiles nothing and leaves a console
#   t_console_source DIR             copy the console's source into a scratch checkout
#   slice_state / slice_child / slice_id   a scratch cgroup tree for lib/slice.sh
#   t_done                           print the tally, exit 1 if anything failed
#
# Everything lives under $T, a scratch directory removed on exit. HOME,
# TEHAS_HOME and TEHAS_PROJECT_HOME point into it, so a test cannot touch the
# real ones.
set -uo pipefail

T_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/tehas-test.XXXXXX")
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin" "$T/calls" "$T/home"
t_count=0
t_failed=0
t_skipped=0
t_name=""
t_out=""
t_rc=0

# Values a minimal valid instance needs.
T_MINIMAL=(TEHAS_REPO=acme/widget TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
  PI_ENSEMBLE_SUBAGENT_MODEL=test-model PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider)

t_case() {
  t_name=$1
  t_count=$((t_count + 1))
}
t_bad() {
  t_failed=$((t_failed + 1))
  echo "FAIL: $t_name: $*" >&2
}
t_eq() { [ "$1" = "$2" ] || t_bad "expected '$2', got '$1'"; }
# A case that cannot run every half of itself on this host says so here, so the
# tally names the part that was never checked instead of reading as a pass.
t_skip() {
  t_skipped=$((t_skipped + 1))
  echo "SKIP: $t_name: $*"
}
t_contains() { case "$1" in *"$2"*) ;; *) t_bad "'$2' is not in: $1" ;; esac; }
t_lacks() { case "$1" in *"$2"*) t_bad "'$2' should not be in: $1" ;; esac; }

t_instance() {
  export TEHAS_INSTANCE_DIR=$T/instance
  mkdir -p "$TEHAS_INSTANCE_DIR"
  chmod 700 "$TEHAS_INSTANCE_DIR"
  if [ "$#" -gt 0 ]; then printf '%s\n' "$@" >"$TEHAS_INSTANCE_DIR/instance.env"; else : >"$TEHAS_INSTANCE_DIR/instance.env"; fi
}

t_stub() {
  local name=$1 body=${2:-}
  {
    echo '#!/usr/bin/env bash'
    printf '%s\n' "printf '%s\\n' \"\$*\" >>\"$T/calls/$name\""
    [ -n "$body" ] && echo "$body"
    :
  } >"$T/bin/$name"
  chmod +x "$T/bin/$name"
}
t_calls() { cat "$T/calls/$1" 2>/dev/null || true; }
# install.sh builds the console with cargo. This one writes a file where the
# build would leave the program: in $CARGO_TARGET_DIR, which t_run points into
# the scratch directory, so no test builds into this checkout. CARGO_RC makes
# the build fail, CONSOLE_BUILD tells one build from another, and NO_CONSOLE
# is a build that succeeds and leaves nothing.
t_stub_cargo() {
  # shellcheck disable=SC2016  # the stub's body is expanded when the stub runs
  t_stub cargo 'if [ "${CARGO_RC:-0}" != 0 ]; then echo "error: could not compile console" >&2; exit "$CARGO_RC"; fi
[ -z "${NO_CONSOLE:-}" ] || exit 0
mkdir -p "${CARGO_TARGET_DIR:?}/release"
echo "a console built from ${CONSOLE_BUILD:-source}" >"$CARGO_TARGET_DIR/release/console"'
}
# The console's source without what a build here left beside it.
t_console_source() {
  mkdir -p "$1/console"
  cp -R "$T_ROOT/console/Cargo.toml" "$T_ROOT/console/Cargo.lock" "$T_ROOT/console/src" "$1/console/"
}

# A scratch cgroup tree for lib/slice.sh, under $T_CGROUP. A test that uses
# it puts TEHAS_CGROUP_ROOT=$T_CGROUP in T_ENV and stubs systemctl and docker
# itself. All sizes are bytes.
#
#   slice_state <full avg60> [<some avg60>]
#       Start again: remove the tree, then write cgroup.controllers with cpu,
#       memory and pids, and tehas.slice/memory.pressure with the two averages
#       (some defaults to 0.00). The slice has no child afterwards.
#   slice_child <id> <anon> <slab_reclaimable> <low> [<oom_kill>]
#       One container in the slice, the directory tehas.slice/docker-<id>.scope
#       with memory.stat, memory.low and memory.events. <id> is the container's
#       id as a cid file holds it, 64 hex characters. <low> is the floor, a
#       number or "max". <oom_kill> defaults to 0. In memory.stat, kernel is
#       <slab_reclaimable> plus $SLICE_KERNEL, file is $SLICE_FILE and shmem is
#       $SLICE_SHMEM; each of the three defaults to 0. Calling it again for the
#       same id writes the files again.
#   slice_id <hex digit>
#       Print a container id: that digit 64 times.
T_CGROUP=$T/cgroup
slice_state() {
  rm -rf "$T_CGROUP"
  mkdir -p "$T_CGROUP/tehas.slice"
  echo "cpuset cpu io memory hugetlb pids rdma misc" >"$T_CGROUP/cgroup.controllers"
  printf '%s\n' "some avg10=0.00 avg60=${2:-0.00} avg300=0.00 total=0" \
    "full avg10=0.00 avg60=$1 avg300=0.00 total=0" >"$T_CGROUP/tehas.slice/memory.pressure"
}
slice_child() {
  local dir=$T_CGROUP/tehas.slice/docker-$1.scope
  mkdir -p "$dir"
  printf '%s\n' "anon $2" "file ${SLICE_FILE:-0}" "kernel $(($3 + ${SLICE_KERNEL:-0}))" "kernel_stack 0" \
    "shmem ${SLICE_SHMEM:-0}" "slab_reclaimable $3" "slab_unreclaimable 0" >"$dir/memory.stat"
  printf '%s\n' "$4" >"$dir/memory.low"
  printf '%s\n' "low 0" "high 0" "max 0" "oom 0" "oom_kill ${5:-0}" "oom_group_kill 0" >"$dir/memory.events"
}
slice_id() { printf "$1%.0s" {1..64}; echo; }

# Run a snippet with a known environment: the stubs first on PATH, scratch
# HOME and TEHAS_HOME, and only the variables named in T_ENV carried over.
T_ENV=()
t_run() {
  t_out=$(env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$T/home/tehas" TEHAS_PROJECT_HOME="$T/home/tehas" T="$T" T_ROOT="$T_ROOT" \
    CARGO_TARGET_DIR="$T/target" \
    ${TEHAS_INSTANCE_DIR:+TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR"} "${T_ENV[@]}" \
    bash -c "$1" 2>&1)
  t_rc=$?
}

t_done() {
  local line
  line="$(basename "$0"): $t_count cases, "
  [ "$t_skipped" = 0 ] || line=$line"$t_skipped skipped, "
  echo "$line$t_failed failed"
  [ "$t_failed" = 0 ]
  exit $?
}
