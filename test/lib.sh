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
#                                    the MCP server, the store and the
#                                    attachment checker into a scratch
#                                    checkout, and name its commit (t_tree_commit)
#   t_tree_commit DIR                write COMMIT, as a release tree carries it
#   t_store                          the real store, built once per run of
#                                    the tests, in $T_STORE; 1 when there is
#                                    none (t_store_why)
#   t_attach_home HOME               the real attachment checker, built once
#                                    per run like the store, installed at
#                                    HOME/libexec/attach; a failure when there
#                                    is none
#   t_store_db HOME                  a fresh database in HOME/state/tehas.db
#   t_store_home HOME                the real store installed at
#                                    HOME/libexec/store and a fresh database,
#                                    and the attachment checker beside it;
#                                    a failure when there is no store
#   t_row VERB ARGS... -- JQ         a read verb of the real store against the
#                                    home of t_store_home, and jq -r JQ of its
#                                    answer
#   t_at EPOCH VERB ARGS...          the real store at that time, against that
#                                    home (TEHAS_STORE_TEST_NOW)
#   t_store_answers VERB RC LINE [TIMES]
#                                    the installed store answers VERB with RC
#                                    and LINE (or never answers: RC hang) for
#                                    its first TIMES calls, and runs the real
#                                    store otherwise; t_calls store
#   t_approve HOME N BY HOW [AT] [ARGS...]
#                                    an approval of issue N on record, as the
#                                    dispatcher records it: at AT [1760000000],
#                                    the body hash of "Words." ($T_WSUM) unless
#                                    ARGS give one, tehas:ready for label and
#                                    fixer
#   t_started HOME N [ARGS...]       a cycle start of issue N on record, made
#                                    under the issue's lock as the cycle makes
#                                    it; TEHAS_STORE_TEST_NOW sets its time
#   t_ended HOME N BY [HOW] [ARGS...]
#                                    a cycle of issue N on record that has
#                                    ended (cycle-end ARGS, status failed
#                                    unless given), started on BY's approval
#                                    of HOW [label], or by hand when BY is -
#   t_record HOME N JSON             on a fresh store, the approval JSON
#                                    describes in the shape the record file had
#                                    (tehas_approval_view's), used by a start of
#                                    issue N: with the person's approval it rests
#                                    on, and the fixer's and the refreshes'
#                                    starts it counts, as ended starts before it
#   t_history_end HOME N END [FIELDS]
#                                    one end of a cycle of issue N on the store
#                                    at END (epoch seconds), with FIELDS in the
#                                    shape the line of history had before the
#                                    store (started [a minute before], rc,
#                                    status, step, cap, park_reason,
#                                    failure_kind, failure_cause, provider,
#                                    parked, pr as an address, by and how; by
#                                    hand without by)
#   t_live HOME N RUNNING STATUS STEP [STARTED [UPDATED]]
#                                    a cycle of issue N as the live view shows
#                                    it: started by hand STARTED seconds ago
#                                    [600], its heartbeat UPDATED seconds ago
#                                    [5 running, else 180] with STEP, and,
#                                    when RUNNING is false, its end then with
#                                    STATUS
#   t_park HOME N REASON [QID|-] [ARGS...]
#                                    the cycle of issue N on record parked on
#                                    REASON (park ARGS), with the park question
#                                    QID on record when one is given
#   t_plan HOME N JSON               on a fresh session of issue N, the planning
#                                    session JSON describes in the shape its
#                                    record file had (tehas_plan_shape): made
#                                    by the planning verbs, as the dispatcher
#                                    makes one. by [alice], label
#                                    [tehas:plan], turns, session [abc-123],
#                                    options and filed of the last turn,
#                                    failed, pending, comment (the open
#                                    question), running (a turn after the
#                                    last), lost, answers (who answered each
#                                    later turn, and whether the fixer did),
#                                    fixer_refused, held and auto; startedAt is when
#                                    the turn that runs began
#   t_plan_end HOME N END [FIELDS]   one end of a first planning turn of issue N
#                                    on the store at END (epoch seconds), with
#                                    FIELDS in the shape its line of history had
#                                    (started [a minute before], rc, status, by
#                                    [alice], label, filed, auto); by hand of nobody
#                                    else, and the session ended unless it was
#                                    finished
#   t_no_old_state HOME              a failure for each old record file in
#                                    HOME that t_old_state names
#   t_no_instance                    drop the instance of t_instance and remove
#                                    it; a failure when the case made none
#   t_after                          a command run as each case ends, by
#                                    t_case and t_done
#   slice_state / slice_child / slice_id   a scratch cgroup tree for lib/slice.sh
#   t_done                           print the tally, exit 1 if anything failed
#
# Everything lives under $T, a scratch directory removed on exit. HOME and
# TEHAS_HOME point into it, so a test cannot touch the real ones. On a factory
# host env.sh exports the settings of lib/config.sh and GH_TOKEN, and
# scripts/test.sh runs a test file with the caller's environment, so they are
# dropped here: what reaches a test file is what the file itself sets. The
# runner's own knobs (TEHAS_TEST_*, T_STORE_*, T_ATTACH_BIN) stay, because scripts/test.sh
# names them and gives them to the file.
set -uo pipefail
# The two families lib/config.sh clears in tehas_load, the PI_RUKAS_* family it
# leaves alone, and the names outside them all.
for t_leak in ${!TEHAS_@} ${!PI_ENSEMBLE_@} ${!PI_RUKAS_@} GH_TOKEN; do
  case $t_leak in
  TEHAS_TEST_*) continue ;;
  esac
  unset -n "$t_leak"
  unset -f "$t_leak"
  unset "$t_leak"
done
unset t_leak

T_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/tehas-test.XXXXXX")
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/bin" "$T/calls" "$T/home"
# The stubs name bash by its path: through /usr/bin/env a stub costs one more
# process each call.
T_BASH=$(command -v bash)
t_count=0
t_failed=0
t_skipped=0
t_name=""
t_out=""
t_rc=0

# Values a minimal valid instance needs.
T_MINIMAL=(TEHAS_REPO=acme/widget TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
  PI_ENSEMBLE_SUBAGENT_MODEL=test-model PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider)

t_after=""
t_case() {
  [ "$t_count" = 0 ] || [ -z "$t_after" ] || eval "$t_after"
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
# t_no_instance: the case is done with the instance its t_instance made: the
# name is dropped, so a run that follows has no instance directory, and the
# directory itself is removed. A case that reaches for "$TEHAS_INSTANCE_DIR"
# without a t_instance fails here instead of removing "".
t_no_instance() {
  if [ -z "${TEHAS_INSTANCE_DIR:-}" ]; then
    t_bad "no instance to remove: the case made none with t_instance"
    return 1
  fi
  local dir=$TEHAS_INSTANCE_DIR
  unset TEHAS_INSTANCE_DIR
  rm -rf "$dir"
}

t_stub() {
  local name=$1 body=${2:-}
  {
    echo "#!$T_BASH"
    printf '%s\n' "printf '%s\\n' \"\$*\" >>\"$T/calls/$name\""
    [ -n "$body" ] && echo "$body"
    :
  } >"$T/bin/$name"
  chmod +x "$T/bin/$name"
}
t_calls() { cat "$T/calls/$1" 2>/dev/null || true; }
# t_hold <file>: another process holds a flock on <file> until t_release.
# The holder is the subshell itself, turned into a sleep, so killing it frees
# the lock at once; t_hold returns once the lock is held.
t_hold() {
  (
    exec 9>"$1" && flock 9 && exec sleep 600
  ) &
  t_holder=$!
  while flock -n "$1" true; do command -p sleep 0.02; done
}
t_release() {
  kill "$t_holder" 2>/dev/null
  wait "$t_holder" 2>/dev/null
}
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
  # fails with the store's own word when STORE_RC is set, at every verb but the
  # dry run install.sh asks before its copies. STORE_BIN names a real
  # store to hand over instead (t_store).
  cat >"$T/store-stand-in" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$T/calls/store"
if [ "${STORE_RC:-0}" != 0 ] && [ "$*" != "import --dry-run" ]; then echo "store: corrupt the database: a stand-in" >&2; exit "$STORE_RC"; fi
case $1 in
  init) echo '{"api":1,"verb":"init","ok":true,"schema":1,"created":true}' ;;
  migrate) printf '{"api":1,"verb":"migrate","ok":true,"from":1,"to":1,"backup":"%s/tehas-1-0.db"}\n' "$3" ;;
  import) dry=false
    [ "${2:-}" != --dry-run ] || dry=true
    printf '{"api":1,"verb":"import","ok":true,"imported":{},"moved":0,"held":[],"dry_run":%s}\n' "$dry" ;;
esac
EOF
}
# The source of the workspace's crates without what a build here left beside
# it.
t_console_source() {
  mkdir -p "$1/console/analytics" "$1/console/attach" "$1/console/core" "$1/console/mcp" "$1/console/store"
  cp -R "$T_ROOT/console/Cargo.toml" "$T_ROOT/console/Cargo.lock" "$T_ROOT/console/src" "$1/console/"
  cp -R "$T_ROOT/console/analytics/Cargo.toml" "$T_ROOT/console/analytics/src" "$1/console/analytics/"
  cp -R "$T_ROOT/console/attach/Cargo.toml" "$T_ROOT/console/attach/src" "$1/console/attach/"
  cp -R "$T_ROOT/console/core/Cargo.toml" "$T_ROOT/console/core/src" "$1/console/core/"
  cp -R "$T_ROOT/console/mcp/Cargo.toml" "$T_ROOT/console/mcp/src" "$1/console/mcp/"
  cp -R "$T_ROOT/console/store/Cargo.toml" "$T_ROOT/console/store/src" "$1/console/store/"
  t_tree_commit "$1"
}
# A scratch tree is no git checkout, so it names its commit as a release does,
# in COMMIT: install.sh refuses a tree whose commit it cannot read.
T_COMMIT=0123456789abcdef0123456789abcdef01234567
t_tree_commit() { printf '%s\n' "$T_COMMIT" >"$1/COMMIT"; }

# t_store: the store program of this checkout, in $T_STORE. scripts/test.sh
# builds it once before it starts the test files and names it in T_STORE_BIN,
# or says in T_STORE_WHY why there is none. A test file run alone builds it
# itself. Both build offline into console/target/shell, not the workspace's
# own target directory: the gate's cargo test holds that one's lock while the
# shell tests run. Returns 1 with the reason in $t_store_why.
T_STORE=$T/store-bin
t_store_why=""
t_store() {
  [ -x "$T_STORE" ] && return 0
  if [ -n "${T_STORE_BIN:-}" ]; then
    cp "$T_STORE_BIN" "$T_STORE" && return 0
    t_store_why="the store built for this run cannot be copied: $T_STORE_BIN"
    return 1
  fi
  if [ -n "${T_STORE_WHY:-}" ]; then
    t_store_why=$T_STORE_WHY
    return 1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    t_store_why="cargo is not installed"
    return 1
  fi
  local target=$T_ROOT/console/target/shell
  if ! cargo build --locked --offline --quiet --manifest-path "$T_ROOT/console/Cargo.toml" \
    --target-dir "$target" -p store >"$T/store-build.log" 2>&1; then
    t_store_why="the store did not build offline: $(tail -n 1 "$T/store-build.log")"
    return 1
  fi
  cp "$target/debug/store" "$T_STORE"
}
# t_attach: the attachment checker of this checkout, in $T_ATTACH, as t_store
# gets the store: from T_ATTACH_BIN, or built here when the file runs alone.
T_ATTACH=$T/attach-bin
t_attach_why=""
t_attach() {
  [ -x "$T_ATTACH" ] && return 0
  if [ -n "${T_ATTACH_BIN:-}" ]; then
    cp "$T_ATTACH_BIN" "$T_ATTACH" && return 0
    t_attach_why="the attachment checker built for this run cannot be copied: $T_ATTACH_BIN"
    return 1
  fi
  if [ -n "${T_STORE_WHY:-}" ]; then
    t_attach_why=$T_STORE_WHY
    return 1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    t_attach_why="cargo is not installed"
    return 1
  fi
  local target=$T_ROOT/console/target/shell
  if ! cargo build --locked --offline --quiet --manifest-path "$T_ROOT/console/Cargo.toml" \
    --target-dir "$target" -p attach >"$T/attach-build.log" 2>&1; then
    t_attach_why="the attachment checker did not build offline: $(tail -n 1 "$T/attach-build.log")"
    return 1
  fi
  cp "$target/debug/attach" "$T_ATTACH"
}
# t_dispatch: tehas-dispatch of this checkout, in $T_DISPATCH, as t_store gets
# the store: from T_DISPATCH_BIN, or built here when the file runs alone.
T_DISPATCH=$T/dispatch-bin
t_dispatch_why=""
t_dispatch() {
  [ -x "$T_DISPATCH" ] && return 0
  if [ -n "${T_DISPATCH_BIN:-}" ]; then
    cp "$T_DISPATCH_BIN" "$T_DISPATCH" && return 0
    t_dispatch_why="tehas-dispatch built for this run cannot be copied: $T_DISPATCH_BIN"
    return 1
  fi
  if [ -n "${T_STORE_WHY:-}" ]; then
    t_dispatch_why=$T_STORE_WHY
    return 1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    t_dispatch_why="cargo is not installed"
    return 1
  fi
  local target=$T_ROOT/console/target/shell
  if ! cargo build --locked --offline --quiet --manifest-path "$T_ROOT/console/Cargo.toml" \
    --target-dir "$target" -p tehas-dispatch >"$T/dispatch-build.log" 2>&1; then
    t_dispatch_why="tehas-dispatch did not build offline: $(tail -n 1 "$T/dispatch-build.log")"
    return 1
  fi
  cp "$target/debug/tehas-dispatch" "$T_DISPATCH"
}
# t_cycle: tehas-cycle of this checkout, in $T_CYCLE, as t_dispatch gets
# tehas-dispatch: from T_CYCLE_BIN, or built here when the file runs alone.
T_CYCLE=$T/cycle-bin
t_cycle_why=""
t_cycle() {
  [ -x "$T_CYCLE" ] && return 0
  if [ -n "${T_CYCLE_BIN:-}" ]; then
    cp "$T_CYCLE_BIN" "$T_CYCLE" && return 0
    t_cycle_why="tehas-cycle built for this run cannot be copied: $T_CYCLE_BIN"
    return 1
  fi
  if [ -n "${T_STORE_WHY:-}" ]; then
    t_cycle_why=$T_STORE_WHY
    return 1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    t_cycle_why="cargo is not installed"
    return 1
  fi
  local target=$T_ROOT/console/target/shell
  if ! cargo build --locked --offline --quiet --manifest-path "$T_ROOT/console/Cargo.toml" \
    --target-dir "$target" -p tehas-cycle >"$T/cycle-build.log" 2>&1; then
    t_cycle_why="tehas-cycle did not build offline: $(tail -n 1 "$T/cycle-build.log")"
    return 1
  fi
  cp "$target/debug/tehas-cycle" "$T_CYCLE"
}
# t_attach_home <home>: the attachment checker where a command runs it.
t_attach_home() {
  if ! t_attach; then
    t_bad "no attachment checker: $t_attach_why"
    return 1
  fi
  mkdir -p "$1/libexec"
  rm -f "$1/libexec/attach"
  ln "$T_ATTACH" "$1/libexec/attach" 2>/dev/null || cp "$T_ATTACH" "$1/libexec/attach"
}
# t_store_db <home>: a fresh database, made by the store's own init, in
# <home>/state/tehas.db. Needs t_store first.
t_store_db() {
  rm -f "$1/state/tehas.db" "$1/state/tehas.db-wal" "$1/state/tehas.db-shm"
  mkdir -p "$1"
  TEHAS_HOME=$1 "$T_STORE" init --version v0.0.0 --commit "$T_COMMIT" --instance instance \
    --repo acme/widget >/dev/null
}
# t_store_home <home>: the home a command runs the store in. A record test
# fails without the store rather than skip: the gate needs cargo anyway. The
# binary is a hard link to $T_STORE, so a case that changes it removes it
# first, as t_store_answers does.
t_store_at=""
t_store_home() {
  t_store_at=$1
  if ! t_store; then
    t_bad "no store: $t_store_why"
    return 1
  fi
  mkdir -p "$1/libexec"
  rm -f "$1/libexec/store" "$T/store-answers.n"
  ln "$T_STORE" "$1/libexec/store" 2>/dev/null || cp "$T_STORE" "$1/libexec/store"
  t_store_db "$1" || t_bad "the store's init failed in $1"
  # A command that runs the store reads an issue's attachments in the same home.
  t_attach_home "$1"
}
# t_row <verb> <args...> -- <jq filter>: what the store has on record, through
# its read verbs only. On a refusal it prints the store's own line and
# returns 1.
t_row() {
  local t_args=() t_got
  case ${1:-} in
    cycle-show | park-show | question-known | approval-current | plan-show | history-show | check) ;;
    *)
      t_bad "t_row reads, and ${1:-nothing} is not a read verb"
      return 64
      ;;
  esac
  while [ "$#" -gt 0 ] && [ "$1" != -- ]; do
    t_args+=("$1")
    shift
  done
  if [ "$#" != 2 ]; then
    t_bad "t_row: no -- <jq filter> after ${t_args[*]}"
    return 64
  fi
  t_got=$(TEHAS_HOME=$t_store_at "$T_STORE" "${t_args[@]}" 2>&1) || {
    printf '%s\n' "$t_got"
    return 1
  }
  jq -r "$2" <<<"$t_got"
}
# t_at <epoch> <verb> <args...>: the store with its clock at <epoch>, which a
# debug build takes from TEHAS_STORE_TEST_NOW.
t_at() {
  TEHAS_HOME=$t_store_at TEHAS_STORE_TEST_NOW=$1 "$T_STORE" "${@:2}"
}
t_q() { printf %q "$1"; }
# t_store_answers <verb> <rc> <line> [<times>]: a store at the home of
# t_store_home that answers <verb> with <rc>, and <line> on stdout for 0 and
# on stderr otherwise, for its first <times> calls (all when not given).
# <rc> hang is a store that never answers. Every other call goes to the real
# store. Each call is noted in t_calls store.
t_store_answers() {
  local t_bin=$t_store_at/libexec/store t_times=${4:-all} t_answer
  case $2 in
    hang) t_answer="exec $(printf %q "$(command -pv sleep)") 600" ;;
    0) t_answer="printf '%s\\n' $(printf %q "$3"); exit 0" ;;
    *) t_answer="printf '%s\\n' $(printf %q "$3") >&2; exit $(printf %q "$2")" ;;
  esac
  rm -f "$t_bin" "$T/store-answers.n"
  mkdir -p "${t_bin%/*}"
  printf '%s\n' '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$*\" >>$(t_q "$T/calls/store")" \
    "if [ \"\${1:-}\" = $(t_q "$1") ]; then" \
    "  n=\$((\$(cat $(t_q "$T/store-answers.n") 2>/dev/null || echo 0) + 1))" \
    "  echo \"\$n\" >$(t_q "$T/store-answers.n")" \
    "  if [ $(t_q "$t_times") = all ] || [ \"\$n\" -le $(t_q "$t_times") ]; then" \
    "    $t_answer" \
    "  fi" \
    "fi" \
    "exec $(t_q "$T_STORE") \"\$@\"" >"$t_bin"
  chmod 755 "$t_bin"
}
T_WSUM=$(printf 'Words.' | sha256sum | cut -d' ' -f1)
t_approve() { # <home> <issue> <by> <how> [<at>] [<approval-record args>...]
  local t_home=$1 t_n=$2 t_by=$3 t_how=$4 t_when=${5:-1760000000} t_args
  if [ "$#" -ge 5 ]; then shift 5; else shift "$#"; fi
  t_args=(approval-record "$t_n" --at "$t_when" --by "$t_by" --how "$t_how")
  case $t_how in label | fixer) t_args+=(--label tehas:ready) ;; esac
  case " $* " in *" --body-sha "*) ;; *) t_args+=(--body-sha "$T_WSUM") ;; esac
  TEHAS_HOME=$t_home "$T_STORE" "${t_args[@]}" "$@" >/dev/null ||
    t_bad "t_approve: the store refused ${t_args[*]} $*"
}
t_started() { # <home> <issue> [<cycle-start args>...]
  local t_home=$1 t_n=$2
  shift 2
  mkdir -p "$t_home/state/cycles"
  TEHAS_HOME=$t_home flock -w 5 "$t_home/state/cycles/$t_n.lock" "$T_STORE" cycle-start "$t_n" "$@" >/dev/null ||
    t_bad "t_started: the store refused cycle-start $t_n $*"
}
t_ended() { # <home> <issue> <by|-> [<how>] [<cycle-end args>...]
  local t_home=$1 t_n=$2 t_by=$3 t_how=${4:-label} t_a
  if [ "$#" -ge 4 ]; then shift 4; else shift "$#"; fi
  t_a=$(TEHAS_HOME=$t_home "$T_STORE" cycle-show "$t_n" | jq '.cycle.attempt // 0')
  if [ "$t_by" = - ]; then
    t_started "$t_home" "$t_n" --by-hand
  else
    t_approve "$t_home" "$t_n" "$t_by" "$t_how" $((1700000000 + t_a))
    t_started "$t_home" "$t_n"
  fi
  case " $* " in *" --status "*) ;; *) set -- --status failed "$@" ;; esac
  TEHAS_HOME=$t_home flock -w 5 "$t_home/state/cycles/$t_n.lock" "$T_STORE" cycle-end "$t_n" --attempt $((t_a + 1)) "$@" >/dev/null ||
    t_bad "t_ended: the store refused cycle-end $t_n $*"
}

t_live() { # <home> <issue> <running> <status> <step> [<started ago>] [<updated ago>]
  local t_home=$1 t_n=$2 t_now t_a t_state=--running t_ago=${7:-5}
  t_now=$(date +%s)
  [ "$3" = true ] || { t_state=--ended; t_ago=${7:-180}; }
  TEHAS_STORE_TEST_NOW=$((t_now - ${6:-600})) t_started "$t_home" "$t_n" --by-hand
  t_a=$(TEHAS_HOME=$t_home "$T_STORE" cycle-show "$t_n" | jq '.cycle.attempt')
  TEHAS_HOME=$t_home TEHAS_STORE_TEST_NOW=$((t_now - t_ago)) "$T_STORE" cycle-heartbeat "$t_n" \
    "$t_state" --status "$4" --step "$5" >/dev/null || t_bad "t_live: the store refused the heartbeat of $t_n"
  [ "$3" = true ] && return 0
  TEHAS_HOME=$t_home TEHAS_STORE_TEST_NOW=$((t_now - t_ago)) flock -w 5 "$t_home/state/cycles/$t_n.lock" \
    "$T_STORE" cycle-end "$t_n" --attempt "$t_a" --status "$4" --step "$5" >/dev/null ||
    t_bad "t_live: the store refused the end of $t_n"
}

t_record() { # <home> <issue> <JSON>
  local t_home=$1 t_n=$2 t_j=$3 t_how t_at t_last=0 t_base t_k t_count t_in=$T/record-input.json
  local -a t_args
  t_store_db "$t_home"
  t_how=$(jq -r .how <<<"$t_j")
  t_at=$(date -u -d "$(jq -r .at <<<"$t_j")" +%s)
  t_base=$(jq -c '.rests_on // empty' <<<"$t_j")
  # _t_used <by> <how> <at> [<approval-record args>...]: an approval a start used, and its end.
  _t_used() {
    local t_a
    t_a=$(TEHAS_HOME=$t_home "$T_STORE" cycle-show "$t_n" | jq '.cycle.attempt // 0')
    t_approve "$t_home" "$t_n" "$@"
    t_started "$t_home" "$t_n"
    TEHAS_HOME=$t_home flock -w 5 "$t_home/state/cycles/$t_n.lock" "$T_STORE" cycle-end "$t_n" --attempt $((t_a + 1)) \
      --status handoff >/dev/null || t_bad "t_record: the store refused the end of an earlier start"
    t_last=$3
  }
  if { [ "$t_how" = fixer ] || [ "$t_how" = refresh ]; } && [ -n "$t_base" ]; then
    _t_used "$(jq -r .by <<<"$t_base")" "$(jq -r .how <<<"$t_base")" "$(date -u -d "$(jq -r .at <<<"$t_base")" +%s)" \
      --body-sha "$(jq -r .body_sha256 <<<"$t_base")"
    t_count=$(jq -r '.fixer_starts // 0' <<<"$t_j")
    [ "$t_how" != fixer ] || t_count=$((t_count - 1))
    for ((t_k = 0; t_k < t_count; t_k++)); do _t_used tehas-fixer fixer $((t_last + 10)); done
    if [ "$t_how" = refresh ]; then
      t_count=$(jq -r '.refreshes // 1' <<<"$t_j")
      for ((t_k = 1; t_k < t_count; t_k++)); do
        _t_used "$(jq -r .by <<<"$t_j")" refresh $((t_last + 10)) --refresh-pr 50 --refresh-head "$(printf 'a%.0s' {1..40})"
      done
    fi
  fi
  [ "$t_at" -gt "$t_last" ] || t_at=$((t_last + 10))
  jq --arg how "$t_how" '{links: (.links // [])}
    + with_entries(select(.key == "comment_links" or .key == "retry_context"))
    + (if $how == "refresh" then {refresh_of: (.refresh_of // null)} else {} end)' <<<"$t_j" >"$t_in"
  t_args=(approval-record "$t_n" --at "$t_at" --by "$(jq -r .by <<<"$t_j")" --how "$t_how"
    --body-sha "$(jq -r .body_sha256 <<<"$t_j")" --input-file "$t_in")
  case $t_how in label | fixer) t_args+=(--label tehas:ready) ;; esac
  t_k=$(jq -r '.title_sha256 // ""' <<<"$t_j")
  [ -z "$t_k" ] || t_args+=(--title-sha "$t_k")
  [ "$t_how" != refresh ] ||
    t_args+=(--refresh-pr "$(jq -r '.refresh_of.pr // 50' <<<"$t_j")" --refresh-head "$(jq -r '.refresh_of.head_sha // ("a" * 40)' <<<"$t_j")")
  TEHAS_HOME=$t_home "$T_STORE" "${t_args[@]}" >/dev/null || t_bad "t_record: the store refused ${t_args[*]}"
  t_started "$t_home" "$t_n"
}

t_history_end() { # <home> <issue> <epoch of the end> [<fields>]
  local home=$1 n=$2 e=$3 x=${4:-'{}'} started a
  local -a t_end=()
  started=$(jq -r --argjson e "$e" '.started // ($e - 60)' <<<"$x")
  a=$(TEHAS_HOME=$home "$T_STORE" cycle-show "$n" | jq '.cycle.attempt // 0')
  if [ "$(jq -r '.by // ""' <<<"$x")" != "" ]; then
    t_approve "$home" "$n" "$(jq -r .by <<<"$x")" "$(jq -r '.how // "label"' <<<"$x")" $((started - 1 - a))
    TEHAS_STORE_TEST_NOW=$started t_started "$home" "$n"
  else
    TEHAS_STORE_TEST_NOW=$started t_started "$home" "$n" --by-hand
  fi
  mapfile -t t_end < <(jq -r '
    "--status", (.status // "unknown"),
    (if .rc != null then ("--rc", (.rc | tostring)) else empty end),
    (if .step != null then ("--step", .step) else empty end),
    (if .cap != null then ("--cap", .cap) else empty end),
    (if .park_reason != null then ("--park-reason", .park_reason) else empty end),
    (if .failure_kind != null then ("--failure-kind", .failure_kind) else empty end),
    (if .failure_cause != null then ("--failure-cause", .failure_cause) else empty end),
    (if .provider == true then "--provider" else empty end),
    (if .parked == true then "--parked" else empty end),
    (if .pr != null then ("--pr", (.pr | sub(".*/"; ""))) else empty end)' <<<"$x")
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$e flock -w 5 "$home/state/cycles/$n.lock" "$T_STORE" cycle-end "$n" \
    --attempt $((a + 1)) "${t_end[@]}" >/dev/null || t_bad "t_history_end: the store refused the end of #$n"
}
t_park() { # <home> <issue> <reason> [<question id>|-] [<park args>...]
  local t_home=$1 t_n=$2 t_why=$3 t_q=${4:--} t_a
  if [ "$#" -ge 4 ]; then shift 4; else shift "$#"; fi
  t_a=$(TEHAS_HOME=$t_home "$T_STORE" cycle-show "$t_n" | jq '.cycle.attempt // 0')
  TEHAS_HOME=$t_home "$T_STORE" park "$t_n" --attempt "$t_a" --reason "$t_why" "$@" >/dev/null ||
    t_bad "t_park: the store refused park $t_n --attempt $t_a --reason $t_why $*"
  [ "$t_q" = - ] || TEHAS_HOME=$t_home "$T_STORE" question-asked "$t_n" --id "$t_q" --kind park >/dev/null ||
    t_bad "t_park: the store refused the question $t_q of #$t_n"
}

t_plan() { # <home> <issue> <JSON>
  local t_home=$1 t_n=$2 t_j=$3 t_turns t_t t_s t_at t_by t_brief=$T/plan-brief.md
  local -a t_args
  _t_s() { TEHAS_HOME=$t_home "$T_STORE" "$@" >/dev/null || t_bad "t_plan: the store refused $*"; }
  # _t_when <epoch or empty> <verb...>: at that time, or now.
  _t_when() {
    local when=$1
    shift
    if [ -n "$when" ]; then TEHAS_HOME=$t_home TEHAS_STORE_TEST_NOW=$when "$T_STORE" "$@" >/dev/null || t_bad "t_plan: the store refused $*"
    else _t_s "$@"; fi
  }
  t_turns=$(jq -r '.turns // 0' <<<"$t_j")
  t_s=$(jq -r '.session // "" | if . == "" then "abc-123" else . end' <<<"$t_j")
  t_at=""
  [ "$(jq -r '.startedAt // ""' <<<"$t_j")" = "" ] || t_at=$(date -u -d "$(jq -r .startedAt <<<"$t_j")" +%s)
  printf '# Faster search\n\nMake it fast.\n' >"$t_brief"
  # Who answered turn <t>, as the dispatcher records it.
  _t_answer() {
    t_by=$(jq -r --argjson t "$1" '[(.answers // [])[] | select(.turn == $t)][0].by // "alice"' <<<"$t_j")
    t_args=(plan-answer "$t_n" --turn "$1" --by "$t_by" --text-file "$t_brief")
    [ "$(jq -r --argjson t "$1" '[(.answers // [])[] | select(.turn == $t)][0].fixer // false' <<<"$t_j")" != true ] ||
      t_args+=(--fixer)
    _t_when "${2:-}" "${t_args[@]}"
  }
  t_args=(plan-begin "$t_n" --by "$(jq -r '.by // "alice"' <<<"$t_j")" --label "$(jq -r '.label // "tehas:plan"' <<<"$t_j")" --brief-file "$t_brief")
  [ "$(jq -r '.auto == true' <<<"$t_j")" = false ] || t_args+=(--auto)
  if [ "$t_turns" = 0 ]; then _t_when "$t_at" "${t_args[@]}"; else _t_s "${t_args[@]}"; fi
  for ((t_t = 1; t_t <= t_turns; t_t++)); do
    [ "$t_t" = 1 ] || _t_answer "$t_t"
    if [ "$t_t" -lt "$t_turns" ]; then
      _t_s plan-end "$t_n" --turn "$t_t" --outcome finished --rc 0 --session "$t_s"
      _t_s plan-end "$t_n" --posted
    elif [ "$(jq -r '.failed // false' <<<"$t_j")" = true ]; then
      _t_s plan-end "$t_n" --turn "$t_t" --outcome failed --rc 2 --session "$t_s"
    else
      t_args=(plan-end "$t_n" --turn "$t_t" --outcome finished --rc 0 --session "$t_s")
      mapfile -t -O "${#t_args[@]}" t_args < <(jq -r '((.options // [])[] | "--option", .), ((.filed // [])[] | "--filed", .)' <<<"$t_j")
      _t_s "${t_args[@]}"
    fi
  done
  if [ "$t_turns" -gt 0 ]; then
    [ "$(jq -r '.comment // ""' <<<"$t_j")" = "" ] ||
      _t_s question-asked "$t_n" --id "$(jq -r .comment <<<"$t_j")" --kind plan
    [ "$(jq -r '.pending // false' <<<"$t_j")" = true ] || _t_s plan-end "$t_n" --posted
    [ "$(jq -r '.running // false' <<<"$t_j")" != true ] || _t_answer $((t_turns + 1)) "$t_at"
  fi
  [ "$(jq -r '.lost // false' <<<"$t_j")" != true ] ||
    _t_s plan-end "$t_n" --turn $((t_turns + 1)) --outcome lost --rc 2
  while read -r t_by; do
    [ -z "$t_by" ] || _t_s plan-note "$t_n" --kind fixer-refused --id "$t_by"
  done < <(jq -r '(.fixer_refused // [])[]' <<<"$t_j")
  while read -r t_by; do
    [ -z "$t_by" ] || _t_s plan-note "$t_n" --kind held --id "$t_by"
  done < <(jq -r '(.held // [])[]' <<<"$t_j")
}
t_plan_end() { # <home> <issue> <epoch of the end> [<fields>]
  local home=$1 n=$2 e=$3 x=${4:-'{}'} started outcome brief=$T/plan-brief.md
  local -a t_end=() t_auto=()
  started=$(jq -r --argjson e "$e" '.started // ($e - 60)' <<<"$x")
  [ "$(jq -r '.auto == true' <<<"$x")" = false ] || t_auto=(--auto)
  outcome=$(jq -r '.status // "finished"' <<<"$x")
  printf '# Faster search\n' >"$brief"
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$started "$T_STORE" plan-begin "$n" --by "$(jq -r '.by // "alice"' <<<"$x")" \
    --label "$(jq -r '.label // "tehas:plan"' <<<"$x")" --brief-file "$brief" "${t_auto[@]}" >/dev/null ||
    t_bad "t_plan_end: the store refused the begin of #$n"
  t_end=(--outcome "$outcome")
  case $outcome in
    finished) t_end+=(--rc 0 --session abc-123) ;;
    failed) t_end+=(--rc "$(jq -r '.rc // 2' <<<"$x")") ;;
  esac
  mapfile -t -O "${#t_end[@]}" t_end < <(jq -r '(.filed // [])[] | "--filed", .' <<<"$x")
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$e "$T_STORE" plan-end "$n" --turn 1 "${t_end[@]}" >/dev/null ||
    t_bad "t_plan_end: the store refused the end of #$n"
}

# The old record files a group moved onto the store no longer has, as globs
# under a home. Each group that moves adds its own.
t_old_state=(
  # The cycle's records: its start, its approval, its image and settings,
  # its branch and its history.
  'state/cycles/*.taken' 'state/cycles/*.by' 'state/cycles/*.requeued' 'state/cycles/*.rukas'
  'state/cycles/*.image' 'state/cycles/*.settings' 'state/cycles/*.branch'
  'state/cycles/*.approved.json' 'state/cycles/*.approved.json.new' 'state/cycles/[1-9]*.history.jsonl'
  # The park, its question and the body of the question posted.
  'state/cycles/*.park' 'state/cycles/*.park.new' 'state/park-*.id' 'state/park-*.json'
  # The quiet-cycle episode.
  'state/cycles/*.quiet'
  # The planning session: its record, its brief and answer, the copy a turn
  # left, the directory it kept in state/ and its history.
  'state/plan-*.json' 'state/plan-*.json.new' 'state/plan-*.json.brief' 'state/plan-*.json.answer'
  'state/plan-*.json.answer.new' 'state/plan-*.json.keep' 'state/plan-*.turn'
  'state/cycles/plan-*.history.jsonl' 'state/plan-*.history.jsonl'
)
t_no_old_state() { # <home>
  local t_glob t_file
  for t_glob in "${t_old_state[@]}"; do
    for t_file in "$1"/$t_glob; do
      if [ -e "$t_file" ] || [ -L "$t_file" ]; then t_bad "an old record file is there: ${t_file#"$1"/}"; fi
    done
  done
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
# HOME and TEHAS_HOME, and only the variables named in T_ENV carried over. Each
# entry of T_ENV is a NAME=VALUE assignment, as env reads them. A file that
# wants another name in a run's environment names it in T_ENV itself; the
# instance directory below is the one exception, and t_instance is the only
# way a file gets one.
T_ENV=()
t_started=0
t_run() {
  # The instance of t_instance goes with the run, and nothing else does: the
  # sweep above took the host's, so a file that names none has the instance
  # resolved from the scratch TEHAS_HOME, as the caller of a run would.
  local -a instance_env=()
  [ -z "${TEHAS_INSTANCE_DIR:-}" ] || instance_env=(TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR")
  t_out=$(env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$T/home/tehas" T="$T" T_ROOT="$T_ROOT" \
    CARGO_TARGET_DIR="$T/target" \
    "${instance_env[@]}" "${T_ENV[@]}" \
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
  [ "$t_count" = 0 ] || [ -z "$t_after" ] || eval "$t_after"
  line="$(basename "$0"): $t_count cases, "
  [ "$t_skipped" = 0 ] || line=$line"$t_skipped skipped, "
  echo "$line$t_failed failed"
  [ "$t_failed" = 0 ]
  exit $?
}
