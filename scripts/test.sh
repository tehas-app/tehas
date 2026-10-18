#!/usr/bin/env bash
# Run the checks of the tehas platform itself.
#
#   1. shellcheck over the shell scripts, at every severity: the call carries no
#      -S floor and excludes no code, so an info note fails the run like any
#      other note. Without shellcheck installed the step is a skipped line and
#      the run passes on the test files alone.
#   2. the shell tests in test/*_test.sh
#
# Where a single quote is the point of a line - an assertion that some text is
# a literal - SC2016 is wrong about it. The manual states the policy for
# suppressions: host/docs/tools.md, under scripts/test.sh.
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
# what never ran is named again after all of it, sized so the block fits the
# end of the output a caller keeps (scripts/verify.sh): it always gives the
# count, always names the lint when it failed, and the test file names that do
# not fit are counted instead.
#
# Exit: 0 every check passed, 1 a check failed, 2 nothing could be checked
set -uo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
lintbad=()  # the lint, when it ran and refused a file
bad=()      # test files that did not pass, by path relative to the checkout
why=()      # why bad[i] is named beyond its path, empty when it is just that
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
    lintbad+=(shellcheck)
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
  # One record per test file, under the same identity the block reports: the
  # path relative to the checkout, so no two files share a record.
  name=${t#"$root"/}
  key=$out/$name
  mkdir -p "${key%/*}" || { echo "test.sh: no record directory for $name" >&2; exit 2; }
  # A file's status is written only when it ends, so a missing one is a failure.
  (bash "$t" >"$key.log" 2>&1; echo "$?" >"$key.rc") &
  running=$((running + 1))
  if [ "$running" -ge "$jobs" ]; then
    wait -n
    running=$((running - 1))
  fi
done
wait
for t in "${tests[@]}"; do
  name=${t#"$root"/}
  key=$out/$name
  reason=
  # A file whose output is gone is a failure even when its status says it
  # passed: the run cannot show what the check said.
  if ! cat "$key.log" 2>/dev/null; then
    echo "${name//[[:cntrl:]]/?}: its output could not be read"
    reason="its output could not be read"
  fi
  rc=
  ok=1
  if [ ! -f "$key.rc" ]; then
    # A status is written only when the file ends, so a missing one is one that
    # never finished.
    ok=0
    reason="it did not finish, no result recorded"
  else
    rc=$(cat "$key.rc" 2>/dev/null)
    if [[ $rc =~ ^[0-9]+$ ]]; then
      [ "$rc" -eq 0 ] || ok=0
    else
      ok=0
      reason="its recorded result could not be read"
    fi
  fi
  if [ "$ok" -eq 0 ] || [ -n "$reason" ]; then
    bad+=("$name")
    why+=("$reason")
  fi
done

# scripts/verify.sh keeps the last 40 lines of this step, and the block is what
# a person of that gate sees of a failing run, so its size is derived from that
# bound: the count line, the lint when it failed, the names that fit, the line
# counting the rest, one line per check that did not run, and two lines spare
# for anything printed after the block. The lint is outside the cap: nothing
# else in the block says the shell scripts were refused.
keep=40
nf=${#bad[@]}
cap=$((keep - 5 - ${#lintbad[@]} - ${#skipped[@]}))
[ "$cap" -ge 1 ] || cap=1
failed=$(( ${#lintbad[@]} + nf ))
[ "$ran" = 1 ] || { echo "test.sh: nothing could be checked" >&2; exit 2; }
if [ "$failed" -gt 0 ] || [ "${#skipped[@]}" -gt 0 ]; then
  printf '\ntest.sh: failed checks (%s), not run (%s)\n' "$failed" "${#skipped[@]}"
  for e in "${lintbad[@]}"; do
    printf '    %s\n' "$e"
  done
  shown=$nf
  [ "$shown" -le "$cap" ] || shown=$cap
  for ((i = nf - shown; i < nf; i++)); do
    # A file name is data from the filesystem: control bytes become ? so one
    # entry cannot print escapes or split into two lines. The reason beside it
    # is this script's own words, so only the name is rewritten.
    entry=${bad[i]//[[:cntrl:]]/?}
    [ -z "${why[i]}" ] || entry=$entry': '${why[i]}
    printf '    %s\n' "$entry"
  done
  hidden=$((nf - shown))
  [ "$hidden" -eq 0 ] || echo "    and $hidden more, in the full log"
  for s in "${skipped[@]}"; do
    printf '    skipped: %s\n' "${s//[[:cntrl:]]/?}"
  done
fi

[ "$failed" -eq 0 ] || exit 1
exit 0
