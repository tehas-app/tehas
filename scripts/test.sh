#!/usr/bin/env bash
# Run the checks of the tehas platform itself.
#
#   1. shellcheck over the shell scripts
#   2. the shell tests in test/*_test.sh
#
# Both need bash, git, jq and shellcheck: no sandbox image, no product
# checkout and no network. It belongs to a checkout of the platform and is not
# installed on a host: there is no test/ directory there.
#
# Usage: scripts/test.sh
#
# The test files run side by side, at most TEHAS_TEST_JOBS at once [the CPU
# quota of the cgroup it runs in, rounded up, or else the number of CPUs; a
# container's quota does not change what nproc says]: each keeps everything
# under a scratch directory of its own
# (test/lib.sh), and one after another they took about 18 minutes. Each file's
# output is printed whole, in name order, once all have ended. What failed and
# what never ran is named again after all of it, capped so the block fits the
# end of the output a caller keeps (scripts/verify.sh): it always gives the
# count, and the names that do not fit are counted instead.
#
# Exit: 0 every check passed, 1 a check failed, 2 nothing could be checked
set -uo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bad=()      # checks that failed: shellcheck, or a test file relative to the checkout
skipped=()  # checks that did not run, with the reason
ran=0

files=()
for f in "$root"/bin/* "$root"/libexec/* "$root"/lib/*.sh "$root"/scripts/*.sh "$root"/test/*.sh \
  "$root/install.sh" "$root/bootstrap.sh" "$root/host/env.sh" "$root/host/sandbox/build.sh" "$root/host/login/tehas"; do
  [ -f "$f" ] || continue
  # publish-lib.sh is a vendored copy, checked where it is kept.
  case "$f" in */scripts/publish-lib.sh) continue ;; esac
  files+=("$f")
done
if ! command -v shellcheck >/dev/null 2>&1; then
  echo "WARN: shellcheck is not installed, skipping it" >&2
  skipped+=("shellcheck is not installed")
elif [ "${#files[@]}" -gt 0 ]; then
  ran=1
  if (cd "$root" && shellcheck -x "${files[@]}"); then
    echo "shellcheck: ${#files[@]} files, ok"
  else
    echo "shellcheck: failed"
    bad+=(shellcheck)
  fi
fi

cpus() {
  local quota period
  if read -r quota period 2>/dev/null </sys/fs/cgroup/cpu.max &&
    [[ $quota =~ ^[1-9][0-9]*$ && $period =~ ^[1-9][0-9]*$ ]]; then
    echo $(((quota + period - 1) / period))
  else
    nproc 2>/dev/null || echo 1
  fi
}
jobs=${TEHAS_TEST_JOBS:-$(cpus)}
[[ $jobs =~ ^[1-9][0-9]*$ ]] || jobs=1
out=$(mktemp -d "${TMPDIR:-/tmp}/tehas-tests.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT
tests=()
for t in "$root"/test/*_test.sh; do
  [ -f "$t" ] && tests+=("$t")
done
running=0
for t in "${tests[@]}"; do
  ran=1
  # A file's status is written only when it ends, so a missing one is a failure.
  (bash "$t" >"$out/${t##*/}.log" 2>&1; echo "$?" >"$out/${t##*/}.rc") &
  running=$((running + 1))
  if [ "$running" -ge "$jobs" ]; then
    wait -n
    running=$((running - 1))
  fi
done
wait
for t in "${tests[@]}"; do
  name=${t#"$root"/}
  cat "$out/${t##*/}.log" 2>/dev/null || echo "$name: its output could not be read"
  rc=$(cat "$out/${t##*/}.rc" 2>/dev/null)
  [ "$rc" = 0 ] && continue
  # A missing .rc means the file never ended. It is a failure, and says so.
  if [ -z "$rc" ]; then
    bad+=("$name: it did not finish, no result recorded")
  else
    bad+=("$name")
  fi
done

# scripts/verify.sh keeps the last 40 lines of this step, so the block is capped
# to fit them whole: header, cap names, the line that counts the rest, and one
# line per check that did not run.
cap=36
failed=${#bad[@]}
[ "$ran" = 1 ] || { echo "test.sh: nothing could be checked" >&2; exit 2; }
if [ "$failed" -gt 0 ] || [ "${#skipped[@]}" -gt 0 ]; then
  printf '\ntest.sh: failed checks (%s), not run (%s)\n' "$failed" "${#skipped[@]}"
  first=$((failed > cap ? failed - cap : 0))
  for ((i = first; i < failed; i++)); do
    # A file name is data from the filesystem: control bytes become ? so the
    # block cannot print escapes or split one entry into two lines.
    printf '    %s\n' "${bad[i]//[^[:print:]]/?}"
  done
  [ "$first" = 0 ] || echo "    and $first more, in the full log"
  for s in "${skipped[@]}"; do
    printf '    skipped: %s\n' "$s"
  done
fi

[ "$failed" -eq 0 ] || exit 1
exit 0
