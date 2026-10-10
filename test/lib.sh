# shellcheck shell=bash
# shellcheck disable=SC2034  # the variables here are read by the test files
# Helpers for the shell tests. Source this from a *_test.sh file.
#
#   t_case "what is being checked"   start a case; later failures name it
#   t_run '<bash snippet>'           run it in a clean environment; sets t_out, t_rc
#   t_eq / t_contains / t_lacks      assertions, they record a failure and go on
#   t_instance KEY=VALUE...          write a scratch instance directory
#   t_stub NAME ['<bash body>']      a stub command that records its arguments
#   t_calls NAME                     what the stub was called with, one call per line
#   t_stub_cargo                     a cargo that compiles nothing and leaves a console
#   t_console_source DIR             copy the console's source into a scratch checkout
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
# build would leave the program, in $CARGO_TARGET_DIR, which the case has to
# pass on: without it the stub would write into this checkout. CARGO_RC makes
# the build fail, CONSOLE_BUILD tells one build from another.
t_stub_cargo() {
  # shellcheck disable=SC2016  # the stub's body is expanded when the stub runs
  t_stub cargo 'if [ "${CARGO_RC:-0}" != 0 ]; then echo "error: could not compile console" >&2; exit "$CARGO_RC"; fi
mkdir -p "${CARGO_TARGET_DIR:?}/release"
echo "a console built from ${CONSOLE_BUILD:-source}" >"$CARGO_TARGET_DIR/release/console"'
}
# The console's source without what a build here left beside it.
t_console_source() {
  mkdir -p "$1/console"
  cp -R "$T_ROOT/console/Cargo.toml" "$T_ROOT/console/Cargo.lock" "$T_ROOT/console/src" "$1/console/"
}

# Run a snippet with a known environment: the stubs first on PATH, scratch
# HOME and TEHAS_HOME, and only the variables named in T_ENV carried over.
T_ENV=()
t_run() {
  t_out=$(env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$T/home/tehas" T="$T" T_ROOT="$T_ROOT" \
    ${TEHAS_INSTANCE_DIR:+TEHAS_INSTANCE_DIR="$TEHAS_INSTANCE_DIR"} "${T_ENV[@]}" \
    bash -c "$1" 2>&1)
  t_rc=$?
}

t_done() {
  echo "$(basename "$0"): $t_count cases, $t_failed failed"
  [ "$t_failed" = 0 ]
  exit $?
}
