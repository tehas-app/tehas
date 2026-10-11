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
# Exit: 0 every check passed, 1 a check failed, 2 nothing could be checked
set -uo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
failed=0
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
elif [ "${#files[@]}" -gt 0 ]; then
  ran=1
  if (cd "$root" && shellcheck -x "${files[@]}"); then
    echo "shellcheck: ${#files[@]} files, ok"
  else
    echo "shellcheck: failed"
    failed=1
  fi
fi

for t in "$root"/test/*_test.sh; do
  [ -f "$t" ] || continue
  ran=1
  bash "$t" || failed=1
done

[ "$ran" = 1 ] || { echo "test.sh: nothing could be checked" >&2; exit 2; }
exit "$failed"
