#!/usr/bin/env bash
# The ceiling on the factory's bash: the lines of bin/, lib/*.sh and
# libexec/ (the files whose first line names bash) may not grow past the
# number in scripts/bash-lines.max. New factory logic goes into the Rust
# workspace under console/; bash glues (AGENTS.md, Style). Install, update
# and bootstrap are outside the count. A person lowers the number when a
# change moves bash to Rust, and raises it only with a reason in the pull
# request.
#
# Usage: scripts/check-bash-lines.sh
# Exit: 0 at or under the ceiling, 1 over it or the ceiling is unreadable
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

max=$(tr -d '[:space:]' <scripts/bash-lines.max 2>/dev/null)
if ! [[ $max =~ ^[0-9]+$ ]]; then
  echo "check-bash-lines: scripts/bash-lines.max holds no number" >&2
  exit 1
fi

files=()
for f in bin/* lib/*.sh libexec/*; do
  [ -f "$f" ] && head -n 1 "$f" | grep -q bash && files+=("$f")
done
lines=0
[ "${#files[@]}" = 0 ] || lines=$(cat "${files[@]}" | wc -l)

if [ "$lines" -gt "$max" ]; then
  echo "check-bash-lines: $lines lines of bash, over the ceiling of $max (scripts/bash-lines.max)."
  echo "Put new logic in the Rust workspace under console/, or take out as many lines as you add."
  exit 1
fi
echo "check-bash-lines: $lines of $max"
