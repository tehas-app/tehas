#!/usr/bin/env bash
# Run the checks of the tehas platform itself.
#
#   1. shellcheck over the shell scripts, at every severity: the call carries no
#      -S floor and excludes no code, so an info note fails the run like any
#      other note. Without shellcheck installed the step is a skipped line and
#      the run passes on the test files alone.
#   2. the shell tests in test/*_test.sh
#
# Before the tests start, the state store (console/store) is built once, offline,
# into console/target/shell: a target directory of its own, because
# scripts/verify.sh runs the workspace's cargo test in console/target at the
# same time. The test files are told where it is in T_STORE_BIN, or why there is
# none in T_STORE_WHY (test/lib.sh, t_store). A test that needs the store fails
# without it.
#
# Where a single quote is the point of a line - an assertion that some text is
# a literal - SC2016 is wrong about it. The manual states the policy for
# suppressions: host/docs/tools.md, under scripts/test.sh.
#
# Both need bash, git, jq and shellcheck, and the tests of the store's callers
# need cargo: no sandbox image, no product checkout and no network. It belongs to a checkout of the platform and is not
# installed on a host: there is no test/ directory there.
#
# Usage: scripts/test.sh
#
# The test files and the lint of each shell script run side by side, at most
# TEHAS_TEST_JOBS at once [the CPUs nproc counts, or the CPU quota of the
# cgroup it runs in, rounded up, when that is less]: each test file keeps
# everything under a scratch directory of its own (test/lib.sh). Each task is
# stopped after TEHAS_TEST_FILE_TIMEOUT seconds [300] and counts as failed,
# named with "timed out". Each file's output is printed whole, in name order,
# once all have ended, then the number of cases and the five slowest files. What failed and
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
  "$root/install.sh" "$root/bootstrap.sh" "$root/host/env.sh" "$root/host/sandbox/build.sh" "$root/host/login/tehas" \
  "$root/host/release/install"; do
  [ -f "$f" ] || continue
  # publish-lib.sh is a vendored copy, checked where it is kept.
  case "$f" in */scripts/publish-lib.sh) continue ;; esac
  files+=("$f")
done
lint=0
if ! command -v shellcheck >/dev/null 2>&1; then
  echo "WARN: shellcheck is not installed, skipping it" >&2
  skipped+=("shellcheck is not installed")
elif [ "${#files[@]}" -gt 0 ]; then
  ran=1
  lint=1
fi

# The CPUs this run may use: the cgroup's quota, rounded up, when it is less
# than the CPUs nproc counts. A container's quota does not change what nproc
# says.
cpus() {
  local quota period n
  n=$(nproc 2>/dev/null) || n=1
  if read -r quota period 2>/dev/null </sys/fs/cgroup/cpu.max &&
    [[ $quota =~ ^[1-9][0-9]*$ && $period =~ ^[1-9][0-9]*$ ]] &&
    [ $(((quota + period - 1) / period)) -lt "$n" ]; then
    n=$(((quota + period - 1) / period))
  fi
  echo "$n"
}
jobs=${TEHAS_TEST_JOBS:-$(cpus)}
[[ $jobs =~ ^[1-9][0-9]*$ ]] || jobs=1
limit=${TEHAS_TEST_FILE_TIMEOUT:-300}
[[ $limit =~ ^[1-9][0-9]*$ ]] || limit=300
out=$(mktemp -d "${TMPDIR:-/tmp}/tehas-tests.XXXXXX") || exit 2
trap 'rm -rf "$out"' EXIT

unset T_STORE_BIN T_STORE_WHY
store_why=""
if [ ! -f "$root/console/Cargo.toml" ]; then
  store_why="this checkout has no console/Cargo.toml"
elif ! command -v cargo >/dev/null 2>&1; then
  store_why="cargo is not installed"
elif cargo build --locked --offline --quiet --manifest-path "$root/console/Cargo.toml" \
  --target-dir "$root/console/target/shell" -p store >"$out/store-build.log" 2>&1; then
  export T_STORE_BIN=$root/console/target/shell/debug/store
else
  store_why="the store did not build offline: $(tail -n 1 "$out/store-build.log")"
fi
[ -z "$store_why" ] || export T_STORE_WHY=$store_why

# The slowest files first, so none of them starts last and holds the run. A
# fixed list, the same on every host; the rest follow in name order, and the
# lint of each file after them, to fill the gaps.
first=(work_ dispatch_ install_ preview_ cycle_ top_ improve_ update_ plan_)
tests=()
for p in "${first[@]}"; do
  for t in "$root"/test/"$p"*_test.sh; do
    [ -f "$t" ] && tests+=("$t")
  done
done
for t in "$root"/test/*_test.sh; do
  [ -f "$t" ] || continue
  [[ " ${tests[*]} " == *" $t "* ]] || tests+=("$t")
done

now_ms() { local t=${EPOCHREALTIME/[.,]/}; echo $((10#$t / 1000)); }

# run <record> <command>...: one task of the pool, bounded by $limit. Its
# status is written only when it ends, so a missing one is a failure; so is
# its time.
running=0
run() {
  local key=$1
  shift
  mkdir -p "${key%/*}" || { echo "test.sh: no record directory for ${key#"$out"/}" >&2; exit 2; }
  # From the checkout: shellcheck -x finds sourced files from there.
  (
    cd "$root" || exit
    s=$(now_ms)
    timeout -k 10 "$limit" "$@" >"$key.log" 2>&1
    echo "$?" >"$key.rc"
    echo $(($(now_ms) - s)) >"$key.ms"
  ) &
  running=$((running + 1))
  if [ "$running" -ge "$jobs" ]; then
    wait -n
    running=$((running - 1))
  fi
}
for t in "${tests[@]}"; do
  ran=1
  # One record per test file, under the same identity the block reports: the
  # path relative to the checkout, so no two files share a record. The block
  # reads the log back by that same path, so one file cannot borrow another's
  # FAIL: line.
  run "$out/${t#"$root"/}" bash "$t"
done
if [ "$lint" = 1 ]; then
  for f in "${files[@]}"; do
    run "$out/lint/${f#"$root"/}" shellcheck -x "$f"
  done
fi
wait

if [ "$lint" = 1 ]; then
  refused=0
  for f in "${files[@]}"; do
    key=$out/lint/${f#"$root"/}
    rc=$(cat "$key.rc" 2>/dev/null) || rc=""
    [ "$rc" = 0 ] && continue
    refused=$((refused + 1))
    cat "$key.log" 2>/dev/null
    case "$rc" in
      124 | 137) echo "shellcheck: ${f#"$root"/} timed out after ${limit}s" ;;
    esac
  done
  if [ "$refused" = 0 ]; then
    echo "shellcheck: ${#files[@]} files, ok"
  else
    echo "shellcheck: failed, $refused of ${#files[@]} files"
    lintbad+=(shellcheck)
  fi
fi

cases=0
slow=()
for t in "${tests[@]}"; do
  name=${t#"$root"/}
  key=$out/$name
  reason=
  # A file whose output is gone is a failure even when its status says it
  # passed: the run cannot show what the check said.
  if ! cat "$key.log" 2>/dev/null; then
    echo "${name//[[:cntrl:]]/?}: its output could not be read"
    reason="its output could not be read"
  else
    n=$(sed -n 's/^.*: \([0-9][0-9]*\) cases, [0-9][0-9]* failed$/\1/p' "$key.log" 2>/dev/null | tail -n 1)
    [ -z "$n" ] || cases=$((cases + n))
  fi
  ms=$(cat "$key.ms" 2>/dev/null) || ms=""
  [[ $ms =~ ^[0-9]+$ ]] && slow+=("$ms $name")
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
      if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then reason="timed out after ${limit}s"; fi
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

# What the run covered and where its time went: the count of cases goes down
# only when cases were removed, and the slowest files are the ones to split.
if [ "${#tests[@]}" -gt 0 ]; then
  echo "test.sh: $cases cases in ${#tests[@]} files, $jobs at a time"
  echo "test.sh: the slowest files:"
  for ((i = 0; i < 5 && i < ${#slow[@]}; i++)); do
    top=0
    for ((j = 1; j < ${#slow[@]}; j++)); do
      [ "${slow[j]%% *}" -le "${slow[top]%% *}" ] || top=$j
    done
    ms=${slow[top]%% *}
    printf '    %3d.%d s  %s\n' "$((ms / 1000))" "$((ms % 1000 / 100))" "${slow[top]#* }"
    slow[top]="-1 ${slow[top]#* }"
  done
fi

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
    # One line per file, so the size of the block and the cap above it do not
    # change with the form of the entry.
    entry=${bad[i]//[[:cntrl:]]/?}
    # The file's own first FAIL: line names the case that did not pass, so the
    # tail scripts/verify.sh keeps says which case it was and not only which
    # file. The marker starts the line and the entry is not indented, so a
    # caller looking for FAILED: finds it in column 1, and the file is still
    # named on the line, so a reader that looks for paths finds it too. The
    # text after the marker is data from the log, not this script's words: it is cut to 200
    # characters and its control bytes become ?, as the name already is, so
    # one case cannot print escapes or take the block's lines with it.
    read -r first < <(sed -n 's/^FAIL: //p' "$out/${bad[i]}.log" 2>/dev/null)
    if [ -n "$first" ]; then
      first=${first:0:200}
      printf 'FAILED: %s: %s\n' "$entry" "${first//[[:cntrl:]]/?}"
      continue
    fi
    # A file with a reason and no FAIL: line of its own keeps the indented
    # path. The reason is this script's own words, so only the name is
    # rewritten.
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
