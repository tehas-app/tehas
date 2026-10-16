#!/usr/bin/env bash
# tehas reads pi-rukas only through the contract its headless runner
# publishes: the --capabilities answer, the --json result and the stable
# NDJSON lines (lib/headless.sh). Not its state file, its worktree layout, its
# event log or its source. This test fails when code that is installed or run
# names one of those again. Tests, the manual in host/docs/ and the records in
# docs/ may: a test writes a state file to show it is not read, and the manual
# tells a person where to look.
# shellcheck disable=SC2016  # the sample is lines of code, not expanded
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

# What a host must not read, as extended regular expressions.
internals=(
  'extension/src'
  '/opt/pi-rukas/extension'
  'work-state'
  '\.worktrees'
  'eventLog'
  'pipelineState'
  'currentStep'
  '\.stateFile'
  'work-driver-'
  'roles\.ts'
)

code() { # every tracked file that is installed or run
  git -C "$T_ROOT" ls-files -- bin libexec lib host console/src install.sh bootstrap.sh scripts ':!host/docs'
}

t_case "no code reads pi-rukas internals"
found=""
while IFS= read -r f; do
  for re in "${internals[@]}"; do
    hits=$(grep -nE -- "$re" "$T_ROOT/$f" 2>/dev/null) || continue
    found+="$f: $hits"$'\n'
  done
done < <(code)
t_eq "$found" ""

t_case "the list covers the code that is installed and run"
t_contains "$(code)" "libexec/work"
t_contains "$(code)" "libexec/cycle"
t_contains "$(code)" "host/sandbox/build.sh"
t_contains "$(code)" "host/sandbox/Dockerfile"
t_contains "$(code)" "console/src/render.rs"

t_case "each pattern finds what it is for"
printf '%s\n' 'jq .pipelineState.status "$w/.pi/work-state/7.json"' 'cd "$w/.worktrees/issue-7-a"' \
  'test -e /opt/pi-rukas/extension/src/work-driver-issue-images.ts' 'jq ".eventLog[]"' >"$T/sample"
for re in "${internals[@]}"; do
  case "$re" in currentStep | '\.stateFile' | 'roles\.ts') continue ;; esac
  grep -qE -- "$re" "$T/sample" || t_bad "the pattern $re finds nothing in the sample"
done

t_done
