#!/usr/bin/env bash
# The ceiling on the factory's bash: the lines of bin/, lib/*.sh and
# libexec/ (the files whose first line names bash) may not grow past the
# number in scripts/bash-lines.max. New factory logic goes into the Rust
# workspace under console/; bash glues (AGENTS.md, Style). Install, update
# and bootstrap are outside the count. A change that takes bash out lowers the
# number to the new count in the same pull request, so the room it made is not
# spent by the next one. A raise is a person's call, and its pull request says
# why the logic cannot go into Rust now and names the issue that takes the
# lines out again.
#
# Over the ceiling, it says how many lines to take out and which files
# changed since the merge base with the default branch, with their deltas.
# Without a merge base it lists the largest files. It reads only local refs.
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

if [ "$lines" -le "$max" ]; then
  echo "check-bash-lines: $lines of $max"
  exit 0
fi

echo "check-bash-lines: $lines lines of bash, over the ceiling of $max (scripts/bash-lines.max)."
echo "Take out $((lines - max)) lines."
echo "A raise of the ceiling says why this cannot go into Rust now, and which issue takes the lines out again."

# The lines a path counts for at the merge base, 0 if it was not counted.
base_lines() { # path
  local first
  case $1 in bin/* | lib/*.sh | libexec/*) ;; *) echo 0 && return ;; esac
  first=$(git show "$base:$1" 2>/dev/null | head -n 1)
  case $first in *bash*) git show "$base:$1" | wc -l ;; *) echo 0 ;; esac
}
now_lines() { # path
  if [ -f "$1" ] && printf '%s\n' "${files[@]}" | grep -qxF -- "$1"; then
    wc -l <"$1"
  else
    echo 0
  fi
}

base='' ref=''
if git rev-parse --git-dir >/dev/null 2>&1; then
  for r in $(git symbolic-ref -q --short refs/remotes/origin/HEAD) origin/main main; do
    if base=$(git merge-base HEAD "$r" 2>/dev/null); then
      ref=$r
      break
    fi
  done
fi

shown=0
if [ -n "$base" ]; then
  report=$(
    {
      git diff --name-only "$base" -- bin lib libexec
      git ls-files --others --exclude-standard -- bin lib libexec
    } | sort -u | while IFS= read -r f; do
      old=$(base_lines "$f")
      new=$(now_lines "$f")
      [ "$old" = "$new" ] || printf '  %+6d %6d  %s\n' "$((new - old))" "$new" "$f"
    done | sort -k1,1gr
  )
  if [ -n "$report" ]; then
    echo "Changed since the merge base with $ref ($(git rev-parse --short "$base")), delta and lines now:"
    echo "$report"
    shown=1
  fi
fi
if [ "$shown" = 0 ]; then
  echo "Largest files:"
  wc -l "${files[@]}" | grep -v ' total$' | sort -nr | head -n 5 | awk '{ printf "  %6d  %s\n", $1, $2 }'
fi
echo "Rule: take out as many lines as you add. Do not raise the ceiling; put new logic in the Rust workspace under console/."
exit 1
