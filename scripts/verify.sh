#!/usr/bin/env bash
# The gate of this repository: what must pass before a change is proposed.
#
#   scripts/verify.sh          the lint for the names of particular
#                              installations and private addresses, the
#                              ceiling on bash lines, shellcheck,
#                              every shell test, the tests of the cargo
#                              workspace, its format and lints (rustfmt,
#                              clippy) and, where node is installed, the
#                              tests of the front page
#   scripts/verify.sh --full   the above, and the front page's tests even
#                              where node is missing
#   scripts/verify.sh --changed  only what the change since the merge base with
#                              origin/main can affect, uncommitted and untracked
#                              files included: the name lint and the bash
#                              ceiling, then the checks
#                              scripts/select-tests.sh names, the tests of
#                              only the crates the change can affect among
#                              them. The gate a
#                              developer runs while working; pi-rukas reads it
#                              from .pi/verify-cmd-fast. When the selection
#                              needs everything, or there is no merge base, it
#                              runs as scripts/verify.sh does.
#
# Needs bash, git, jq, shellcheck and cargo; --full also needs node. Without
# cargo the gate fails, fast or full: the dispatcher's rules are Rust tests. A
# missing node is a "skip" line in the fast gate and a failure in the full one;
# rustfmt and clippy, which not every toolchain holds, are "skip" lines
# except in CI and with --full, where they fail. A step that checked less
# than it names is a "skip" line too: the name lint without
# scripts/check-generic.names, and the shell step whose runner reported a
# check it did not run. The line of the name lint says what it did not check
# by its own name, `the names in platform files`. No
# network, no sandbox image, no product checkout: except that cargo fetches the
# workspace's crates the first time it builds it. The workspace's tests run at
# the same time as the shell tests; each step's output is printed in the order
# below. Prints one line per step. The workspace's tests get an empty TMPDIR
# of their own, and anything they leave in it fails the gate: a test makes
# its directories with tehas_core::testdir::TestDir, which removes them.
#
# Every run keeps the whole output of its steps in a log of its own,
# $TEHAS_VERIFY_LOGS/<mode>-<UTC time>-<pid>.log (default
# ${TMPDIR:-/tmp}/tehas-verify; the newest 20 are kept), and a run that failed
# names it on its last line, "log: <path>": a failure's lines show only the
# tail of a step.
#
# Exit: 0 everything passed, 1 a step failed.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1
failed=0
full=0
changed=0
case "${1:-}" in
  --full) full=1 ;;
  --changed) changed=1 ;;
esac

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
mode=${1:-}
mode=${mode#--}
mode=${mode:-plain}

# A tree that passed is not checked again: the same files, tracked or not, in
# the same mode, from the same merge base for --changed, with the same tools.
# git add -A does not see the names list, which .gitignore keeps out of the
# tree, nor CI, so both are hashed here: a pass made without the list is not
# replayed once it is there, nor a plain one under CI. rustfmt and clippy are
# hashed with the rest of the tools: a pass made on a toolchain without them
# says nothing about a run that needs them.
# The pass is kept in the repository's git directory, so its worktrees share
# it and hosts do not. A failure is never kept, nor a pass of a tree that
# changed during the run. TEHAS_VERIFY_FRESH=1 runs every step.
tree_key() {
  local idx=$tmp/index t
  # -p: git rechecks a file not older than the index, so the copy keeps its time.
  cp -p "$(git rev-parse --git-path index 2>/dev/null)" "$idx" 2>/dev/null || return 1
  GIT_INDEX_FILE=$idx git add -A 2>/dev/null || return 1
  {
    echo "${1:-plain}"
    GIT_INDEX_FILE=$idx git write-tree
    [ "$changed" = 0 ] || git merge-base HEAD origin/main 2>/dev/null
    # The list itself, which git add -A cannot see, and CI, which is no file.
    # A file and its contents cannot print the same line: a list whose first
    # line is '42' hashes to an object id, which is no 'names 42'.
    echo "names ${CI-}"
    [ -f scripts/check-generic.names ] && git hash-object scripts/check-generic.names
    for t in bash jq shellcheck cargo rustc node; do
      ! command -v "$t" >/dev/null || "$t" --version 2>&1 | head -n 1
    done
    for t in fmt clippy; do
      ! command -v cargo >/dev/null || cargo "$t" --version 2>&1 | head -n 1
    done
  } | git hash-object --stdin
}
if [ -z "${TEHAS_VERIFY_FRESH:-}" ] && key=$(tree_key "${1:-}") && [ -n "$key" ] &&
  common=$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
  kept=$common/tehas-verify
  if [ -f "$kept/$key" ]; then
    echo "note  this tree passed already; the lines of that run:"
    cat "$kept/$key"
    exit 0
  fi
  TEHAS_VERIFY_FRESH=1 "$root/scripts/verify.sh" "$@" | tee "$tmp/lines"
  rc=${PIPESTATUS[0]}
  if [ "$rc" = 0 ] && [ "$(tree_key "${1:-}")" = "$key" ] && mkdir -p "$kept"; then
    find "$kept" -type f -mtime +7 -delete 2>/dev/null
    cp "$tmp/lines" "$kept/.$key" && mv "$kept/.$key" "$kept/$key"
  fi
  exit "$rc"
fi

# Why a step that ended 0 did not check everything it names, or empty when it
# checked it all. Only the two steps that can under-check ask: the name lint,
# which checks addresses alone without its list, and the shell step, whose
# runner says SKIPPED: <reason> in column 1 for a check it did not run. The
# step's own words stay in the log; the line gives the short reason.
report_skip() { # kind output-file
  case $1 in
    names)
      grep -q '^check-generic: no names list' "$2" && echo "no names list"
      ;;
    shell)
      local reasons
      reasons=$(grep '^SKIPPED: ' "$2" | sed 's/^SKIPPED: //' | paste -sd ',' -)
      [ -n "$reasons" ] && echo "${reasons//,/, }"
      ;;
  esac
  return 0
}

report() { # name rc output-file [why]
  { echo "== $1: exit $2" && cat "$3" && echo; } >>"$tmp/log" 2>/dev/null
  if [ "$2" != 0 ]; then
    echo "FAIL  $1"
    tail -n 40 "$3" | sed 's/^/      /'
    failed=1
    return
  fi
  if [ -n "${4:-}" ]; then
    echo "skip  $1 ($4)"
  else
    echo "ok    $1"
  fi
}

step() { # name command...
  local name=$1 rc=0 why
  shift
  "$@" >"$tmp/out" 2>&1 || rc=$?
  case $name in
    "the names in platform files") why=$(report_skip names "$tmp/out") ;;
    "shellcheck"*) why=$(report_skip shell "$tmp/out") ;;
    *) why= ;;
  esac
  report "$name" "$rc" "$tmp/out" "$why"
}
has() { command -v "$1" >/dev/null 2>&1; }

# --changed: the paths the change touched and what scripts/select-tests.sh
# makes of them. A selection that needs everything turns into the plain gate.
cargo_wanted=1 tests=() nodes=() paths=() omitted=() crates=()
if [ "$changed" = 1 ]; then
  sel=
  if ! base=$(git merge-base HEAD "origin/main" 2>/dev/null); then
    sel="full there is no merge base with origin/main"
  else
    mapfile -t touched < <({ git diff --name-only "$base" && git ls-files --others --exclude-standard; } | sort -u)
    for p in "${touched[@]}"; do
      [ -e "$p" ] && paths+=("$p")
    done
    [ "${#touched[@]}" = 0 ] || sel=$(scripts/select-tests.sh "${touched[@]}") || sel="full the selection failed"
  fi
  if [[ $sel == full\ * ]]; then
    echo "note  the full gate: ${sel#full }"
    changed=0
  else
    cargo_wanted=0 whole=0
    while read -r kind f; do
      case "$kind" in
        cargo) cargo_wanted=1 whole=1 ;;
        crate) cargo_wanted=1 crates+=("$f") ;;
        test) tests+=("$f") ;;
        node) nodes+=("$f") ;;
        omit) omitted+=("${f#test/}") ;;
      esac
    done <<<"$sel"
    [ "$whole" = 0 ] || crates=()
    what=
    [ "$cargo_wanted" = 0 ] || what=", the workspace"
    [ "${#crates[@]}" = 0 ] || what=", the crates ${crates[*]}"
    echo "note  ${#touched[@]} changed paths since ${base:0:12}: ${#tests[@]} test files, ${#nodes[@]} front page files$what"
    [ "${#omitted[@]}" = 0 ] || echo "note  left to the full gate: ${omitted[*]}"
  fi
fi

# The workspace's root manifest is console/Cargo.toml.
workspace=(--manifest-path console/Cargo.toml --workspace)
tested=("${workspace[@]}")
cargo_step="the workspace's tests"
if [ "${#crates[@]}" -gt 0 ]; then
  tested=(--manifest-path console/Cargo.toml)
  for f in "${crates[@]}"; do tested+=(-p "$f"); done
  cargo_step="the tests of ${crates[*]}"
fi
# The tests run with a TMPDIR of their own, which must be empty after them.
cargo_pid="" cargo_tmp=$tmp/cargo-tmp
if has cargo && [ "$cargo_wanted" = 1 ]; then
  mkdir -p "$cargo_tmp"
  TMPDIR=$cargo_tmp cargo test --locked --quiet "${tested[@]}" >"$tmp/cargo" 2>&1 &
  cargo_pid=$!
fi

step "the names in platform files" scripts/check-generic.sh
step "the bash under its ceiling (scripts/bash-lines.max)" scripts/check-bash-lines.sh
if [ "$changed" = 0 ]; then
  step "shellcheck and the shell tests" scripts/test.sh
elif [ "${#tests[@]}" -gt 0 ] || [ "${#paths[@]}" -gt 0 ]; then
  step "shellcheck and the shell tests of the change" scripts/test.sh "${tests[@]}" "${paths[@]}"
fi
if [ "$cargo_wanted" = 0 ]; then
  echo "skip  the workspace's tests (the change touches nothing under console/)"
elif [ -n "$cargo_pid" ]; then
  rc=0
  wait "$cargo_pid" || rc=$?
  report "$cargo_step" "$rc" "$tmp/cargo"
  find "$cargo_tmp" -mindepth 1 -maxdepth 1 >"$tmp/left" 2>&1
  rc=0
  [ ! -s "$tmp/left" ] || rc=1
  report "nothing left in the TMPDIR of cargo test" "$rc" "$tmp/left"
else
  echo "FAIL  $cargo_step (cargo is not installed)"
  failed=1
fi
if [ "$changed" = 1 ] && [ "${#nodes[@]}" = 0 ]; then
  echo "skip  the front page's tests (no test of the change)"
elif [ "$changed" = 1 ] && has node; then
  step "the front page's tests of the change" node --test "${nodes[@]}"
elif has node; then
  step "the front page's tests" node --test test/*_test.mjs
elif [ "$full" = 1 ]; then
  echo "FAIL  the front page's tests (node is not installed)"
  failed=1
else
  echo "skip  the front page's tests (node is not installed; --full needs it)"
fi
# Format and lints run on every gate, not only --full: tehas setup's product
# check runs --full, so a change that passed a shorter gate stopped setup on
# every factory that builds tehas (#1468, #1473, and again on 2026-11-04). In CI or
# with --full a missing rustfmt or clippy fails rather than skips.
strict=$full
[ -n "${CI:-}" ] && strict=1
if has cargo && cargo fmt --version >/dev/null 2>&1; then
  step "the workspace's format" cargo fmt --check --manifest-path console/Cargo.toml --all
elif [ "$strict" = 1 ]; then
  echo "FAIL  the workspace's format (rustfmt is not installed)"
  failed=1
else
  echo "skip  the workspace's format (rustfmt is not installed)"
fi
if has cargo && cargo clippy --version >/dev/null 2>&1; then
  step "the workspace's lints" cargo clippy --locked --quiet --all-targets "${workspace[@]}" -- -D warnings
elif [ "$strict" = 1 ]; then
  echo "FAIL  the workspace's lints (clippy is not installed)"
  failed=1
else
  echo "skip  the workspace's lints (clippy is not installed)"
fi
logs=${TEHAS_VERIFY_LOGS:-${TMPDIR:-/tmp}/tehas-verify}
log=$logs/$mode-$(date -u +%Y%m%dT%H%M%SZ)-$$.log
if mkdir -p "$logs" && cp "$tmp/log" "$log" 2>/dev/null; then
  find "$logs" -maxdepth 1 -name '*.log' -printf '%T@ %p\n' | sort -rn | tail -n +21 | cut -d' ' -f2- | xargs -r rm -f
  [ "$failed" = 0 ] || echo "log: $log"
fi
exit "$failed"
