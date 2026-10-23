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
#   t_stub_analytics ['<bash body>'] a stub of the analytics program where the
#                                    commands look for it, in the scratch
#                                    TEHAS_HOME; t_calls analytics
#   t_fast <ms>                      the last t_run took under <ms> milliseconds;
#                                    call t_start before it
#   t_stub_cargo                     a cargo that compiles nothing and leaves the
#                                    package it was asked for (-p), under its
#                                    target's directory when given --target;
#                                    CARGO_OFFLINE is what a build that asked
#                                    for --offline answers: miss, noise or fail
#   t_console_source DIR             copy the source of the console, analytics,
#                                    the MCP server and the store into a scratch
#                                    checkout, and name its commit (t_tree_commit)
#   t_tree_commit DIR                write COMMIT, as a release tree carries it
#   t_store                          build the real store once per run, in
#                                    $T_STORE; 1 when it cannot (t_store_why)
#   t_store_db HOME                  a fresh database in HOME/state/tehas.db
#   slice_state / slice_child / slice_id   a scratch cgroup tree for lib/slice.sh
#   t_done                           print the tally, exit 1 if anything failed
#
# Everything lives under $T, a scratch directory removed on exit. HOME and
# TEHAS_HOME point into it, so a test cannot touch the real ones.
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
t_stub_analytics() {
  t_stub analytics "${1:-}"
  mkdir -p "$T/home/tehas/libexec"
  mv "$T/bin/analytics" "$T/home/tehas/libexec/analytics"
}
# install.sh builds the console and analytics with cargo, one package (-p) a
# call. This one writes a file where the build would leave the program: in
# $CARGO_TARGET_DIR, which t_run points into the scratch directory, so no test
# builds into this checkout. CARGO_RC makes the build fail, CARGO_FAIL names
# the one package whose build fails, CONSOLE_BUILD tells one build from
# another, and NO_CONSOLE is a build that succeeds and leaves nothing. A build
# that asks for --offline is a build from the cache alone: CARGO_OFFLINE names
# what such a build answers, in cargo's own words, and the build without it is
# The answer is one of miss, fetch_miss, noise or fail, and no other word is
# accepted: an unknown one stops the stub at once rather than answering as a
# cache miss. The knobs compose, so CARGO_RC, CARGO_FAIL and NO_CONSOLE still
# answer the build that goes online.
#   miss  a crate that is not in the cache. The words are what cargo prints for
#         it, captured from a real run against console/Cargo.lock with an empty
#         CARGO_HOME: "error: no matching package named ... found" with its
#         note that offline mode caused it. install.sh retries on this
#   fetch_miss the same miss in cargo's other wording, the one a locked build
#         prints when the fetch itself is refused: "error: ... attempting to
#         make an HTTP request, but --offline was specified". install.sh
#         retries on this too
#   noise words that quote --offline but are no cache miss: a build script that
#         echoes its own command line, as cargo's own usage dump does. install.sh
#         is not to take these for a miss
#   fail a cache miss the fetch does not fix: the offline attempt misses and the
#         retry fails for its own reason, so both are reported. The retry's own
#         words are invented here, not captured from a real cargo: they only
#         exercise the report of both attempts
# Any other build is an ordinary build.
t_stub_cargo() {
  # shellcheck disable=SC2016  # the stub's body is expanded when the stub runs
  t_stub cargo 'pkg=console triple="" prev="" offline=""
for a in "$@"; do
  [ "$prev" != -p ] || pkg=$a
  [ "$prev" != --target ] || triple=$a/
  prev=$a
  [ "$a" != --offline ] || offline=1
done
if [ -n "$offline" ] && [ -n "${CARGO_OFFLINE:-}" ]; then
  # The cache miss in the words cargo uses, one line each. The second line of
  # the miss names offline mode without being an error, so a line that does not
  # open with error: is no miss. The words install.sh matches for a miss are
  # these, named once there in cargo_miss_words and copied here.
  case $CARGO_OFFLINE in
    miss) echo "error: no matching package named \"$pkg\" found" >&2
      echo "note: offline mode (via --offline) can sometimes cause surprising resolution failures" >&2 ;;
    fetch_miss) echo "error: failed to get \"$pkg\": attempting to make an HTTP request, but --offline was specified" >&2 ;;
    noise) echo "error: failed to run custom build command for $pkg" >&2
      echo "  process did not exit successfully: build-script-build --offline" >&2 ;;
    fail) echo "error: no matching package named \"$pkg\" found" >&2 ;;
    *)
      echo "test stub: unknown CARGO_OFFLINE=$CARGO_OFFLINE" >&2
      exit 99
      ;;
  esac
  exit 101
fi
if [ "${CARGO_RC:-0}" != 0 ]; then echo "error: could not compile $pkg" >&2; exit "$CARGO_RC"; fi
if [ "${CARGO_FAIL:-}" = "$pkg" ]; then echo "error: could not compile $pkg" >&2; exit 101; fi
if [ "${CARGO_OFFLINE:-}" = fail ]; then echo "error: could not compile $pkg after fetching" >&2; exit 101; fi
[ -z "${NO_CONSOLE:-}" ] || exit 0
mkdir -p "${CARGO_TARGET_DIR:?}/${triple}release"
if [ "$pkg" = store ]; then
  out=$CARGO_TARGET_DIR/${triple}release/store
  if [ -n "${STORE_BIN:-}" ]; then cp "$STORE_BIN" "$out"; exit 0; fi
  { cat "$T/store-stand-in"; echo "# a store built from ${CONSOLE_BUILD:-source}"; } >"$out"
  chmod 755 "$out"
  exit 0
fi
case $pkg in analytics) art=an ;; *) art=a ;; esac
echo "$art $pkg built from ${CONSOLE_BUILD:-source}" >"$CARGO_TARGET_DIR/${triple}release/$pkg"'
  # The store is run by install.sh, so what the stub builds for it is a program:
  # it records each call in t_calls store and answers as the store does, or
  # fails with the store's own word when STORE_RC is set. STORE_BIN names a real
  # store to hand over instead (t_store).
  cat >"$T/store-stand-in" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/store"
if [ "${STORE_RC:-0}" != 0 ]; then echo "store: corrupt the database: a stand-in" >&2; exit "$STORE_RC"; fi
case $1 in
  init) echo '{"api":1,"verb":"init","ok":true,"schema":1,"created":true}' ;;
  migrate) printf '{"api":1,"verb":"migrate","ok":true,"from":1,"to":1,"backup":"%s/tehas-1-0.db"}\n' "$3" ;;
esac
EOF
}
# The source of the console and of analytics without what a build here left
# beside it.
t_console_source() {
  mkdir -p "$1/console/analytics" "$1/console/mcp" "$1/console/store"
  cp -R "$T_ROOT/console/Cargo.toml" "$T_ROOT/console/Cargo.lock" "$T_ROOT/console/src" "$1/console/"
  cp -R "$T_ROOT/console/analytics/Cargo.toml" "$T_ROOT/console/analytics/src" "$1/console/analytics/"
  cp -R "$T_ROOT/console/mcp/Cargo.toml" "$T_ROOT/console/mcp/src" "$1/console/mcp/"
  cp -R "$T_ROOT/console/store/Cargo.toml" "$T_ROOT/console/store/src" "$1/console/store/"
  t_tree_commit "$1"
}
# A scratch tree is no git checkout, so it names its commit as a release does,
# in COMMIT: install.sh refuses a tree whose commit it cannot read.
T_COMMIT=0123456789abcdef0123456789abcdef01234567
t_tree_commit() { printf '%s\n' "$T_COMMIT" >"$1/COMMIT"; }

# t_store: the store program of this checkout, built once per run of the tests
# and kept in $T_STORE. cargo builds it offline into the workspace's own target
# directory, where the gate's cargo test builds the same crates. Returns 1 with
# the reason in $t_store_why when there is no cargo or the build fails, and the
# case then says t_skip.
T_STORE=$T/store-bin
t_store_why=""
t_store() {
  [ -x "$T_STORE" ] && return 0
  if ! command -v cargo >/dev/null 2>&1; then
    t_store_why="cargo is not installed"
    return 1
  fi
  local target=${CARGO_TARGET_DIR:-$T_ROOT/console/target}
  if ! cargo build --locked --offline --quiet --manifest-path "$T_ROOT/console/Cargo.toml" \
    --target-dir "$target" -p store >"$T/store-build.log" 2>&1; then
    t_store_why="the store did not build offline: $(tail -n 1 "$T/store-build.log")"
    return 1
  fi
  cp "$target/debug/store" "$T_STORE"
}
# t_store_db <home>: a fresh database, made by the store's own init, in
# <home>/state/tehas.db. Needs t_store first.
t_store_db() {
  rm -f "$1/state/tehas.db" "$1/state/tehas.db-wal" "$1/state/tehas.db-shm"
  mkdir -p "$1"
  TEHAS_HOME=$1 "$T_STORE" init --version v0.0.0 --commit "$T_COMMIT" --instance instance \
    --repo acme/widget >/dev/null
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
t_started=0
t_run() {
  t_out=$(env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$T/home/tehas" T="$T" T_ROOT="$T_ROOT" \
    CARGO_TARGET_DIR="$T/target" \
    ${TEHAS_INSTANCE_DIR:+TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR"} "${T_ENV[@]}" \
    bash -c "$1" 2>&1)
  t_rc=$?
}

# A wall-clock bound, in the one place the two timed cases and the one of the
# measure share it. t_start before the run, t_fast after it: the bound is read
# off t_start, not off a start of the case's own, so no case invents its own
# failure message. A slow run says how slow it was.
t_start() { t_started=$(date +%s%N); }
t_fast() { # <milliseconds>
  local elapsed
  elapsed=$((($(date +%s%N) - t_started) / 1000000))
  t_eq "$([ "$elapsed" -lt "$1" ] && echo fast || echo "slow: ${elapsed}ms")" fast
}

# A command that holds the update lock and expects a refusal names the wait: 0
# means refuse at once, so the case is not timed against the default of 10
# seconds. It is exported inside the snippet rather than passed in T_ENV, because
# t_run builds the snippet's environment with env -i: a shell variable of the
# snippet would not reach the command, which would then wait the default for a
# refusal that was meant to come at once.
no_wait="export TEHAS_UPDATE_LOCK_WAIT=0; "

t_done() {
  local line
  line="$(basename "$0"): $t_count cases, "
  [ "$t_skipped" = 0 ] || line=$line"$t_skipped skipped, "
  echo "$line$t_failed failed"
  [ "$t_failed" = 0 ]
  exit $?
}
