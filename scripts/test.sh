#!/usr/bin/env bash
# Run the checks of the tehas platform itself.
#
#   1. shellcheck over the shell scripts, at every severity: the call carries no
#      -S floor and excludes no code, so an info note fails the run like any
#      other note. Without shellcheck installed the step is a SKIPPED: line
#      and the run passes on the test files alone, except when CI or
#      TEHAS_TEST_STRICT is set: then the lint counts as failed.
#   2. the shell tests in test/*_test.sh. A file passes when it exits 0 and
#      its output has the count line of t_done; one that exits 0 without it
#      ended early and fails, named with "ended before t_done".
#
# Before the tests start, the state store (console/store), the attachment
# checker (console/attach), tehas-dispatch (console/dispatch), tehas-cycle
# (console/cycle) and the console (console/src) are built
# once, offline, into the workspace's own target directory, CARGO_TARGET_DIR or
# console/target, with its dev profile
# (when cargo's cache lacks a locked crate, it is fetched once and built again):
# the gate's cargo test (scripts/verify.sh) builds the same crates there, and
# cargo lets go of the directory's lock once a build ends. The test files are
# told where they are in T_STORE_BIN, T_ATTACH_BIN, T_DISPATCH_BIN,
# T_CYCLE_BIN and T_CONSOLE_BIN, or why there are none in T_STORE_WHY
# (test/lib.sh, t_store, t_attach, t_dispatch, t_cycle and t_console). A test that needs one fails without it.
#
# Where a single quote is the point of a line - an assertion that some text is
# a literal - SC2016 is wrong about it. The manual states the policy for
# suppressions: host/docs/tools.md, under scripts/test.sh.
#
# Both need bash, git, jq and shellcheck, and the tests of the store's callers
# need cargo: no sandbox image, no product checkout and no network. It belongs to a checkout of the platform and is not
# installed on a host: there is no test/ directory there.
#
# Usage: scripts/test.sh [path...]
#
# With paths, relative to the root of the checkout, only those run: the test
# files among them, and the lint of those of them it would lint anyway.
# scripts/verify.sh --changed gives it the paths a change touched. A test file
# named that does not exist stops the run with 2.
#
# The test files and the lint of each shell script run side by side, at most
# TEHAS_TEST_JOBS at once [the CPUs nproc counts, or the CPU quota of the
# cgroup it runs in, rounded up, when that is less, and at most two a GiB of
# the cgroup's memory ceiling]: each test file keeps
# everything under a scratch directory of its own (test/lib.sh).
# While the host's load is its CPUs or more, from other runners or a build,
# the next task waits for one to end; one task always runs, so the run goes
# on. TEHAS_TEST_LOADAVG names the file read in place of /proc/loadavg. Each
# task is stopped after TEHAS_TEST_FILE_TIMEOUT seconds [300] and counts as failed,
# named with "timed out". Each file's output is printed whole, in name order,
# once all have ended, then the number of cases and the five slowest files. What failed and
# what never ran is named again after all of it, sized so the block fits the
# end of the output a caller keeps (scripts/verify.sh): it always gives the
# count, always names the lint when it failed, and the test file names that do
# not fit are counted instead.
#
# The work is `xtask test` (console/xtask/src/run.rs). Unless TEHAS_XTASK
# names a build, it is built here first, offline, into the same target
# directory (fetched once when cargo's cache lacks a locked crate), so the run
# needs cargo. Each task of the pool is an `xtask test-task` process that runs
# the check under timeout and records its status and time when it ends.
#
# Exit: 0 every check passed, 1 a check failed, 2 nothing could be checked,
#       or the runner did not build
set -uo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 2
xtask=${TEHAS_XTASK:-}
if [ -z "$xtask" ]; then
  command -v cargo >/dev/null 2>&1 || {
    echo "test.sh: cargo is not installed, and it builds the runner (console/xtask)" >&2
    exit 2
  }
  target=${CARGO_TARGET_DIR:-$root/console/target}
  build() {
    cargo build --locked --offline --quiet --manifest-path "$root/console/Cargo.toml" \
      --target-dir "$target" -p tehas-xtask
  }
  build 2>/dev/null || { cargo fetch --locked --quiet --manifest-path "$root/console/Cargo.toml" && build; } || {
    echo "test.sh: the runner (console/xtask) did not build" >&2
    exit 2
  }
  xtask=$target/debug/xtask
fi
exec "$xtask" test --root "$root" "$@"
