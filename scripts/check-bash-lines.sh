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
# Every run prints the count, the ceiling and the room left or the lines
# over, so a cycle can see how far it has to go without rerunning the check.
# With a merge base that line names the base's own count, the room under it
# and what the change adds on top, so a run can plan its bash before it
# writes any; the working tree's figure is the one the ceiling is judged by.
# When the tree differs from HEAD the line says the figure includes
# uncommitted work. Over the ceiling it adds the files that changed since the
# merge base, with their deltas, and a `pay from` list: the five largest
# counted files the change did not touch, where the lines can come from.
# Without a merge base it lists the largest files. It reads only local refs.
#
# Usage: scripts/check-bash-lines.sh
# Exit: 0 at or under the ceiling, 1 over it or the ceiling is unreadable
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

max=$(tr -d '[:space:]' <scripts/bash-lines.max 2>/dev/null)
# Eighteen digits: one more overflows a signed 64-bit integer and the ceiling
# would read as a negative number, which no tree can be under.
if ! [[ $max =~ ^[0-9]{1,18}$ ]]; then
  echo "check-bash-lines: scripts/bash-lines.max holds no number" >&2
  exit 1
fi

files=()
for f in bin/* lib/*.sh libexec/*; do
  [ -f "$f" ] && head -n 1 "$f" | grep -q bash && files+=("$f")
done
lines=0
[ "${#files[@]}" = 0 ] || lines=$(cat "${files[@]}" | wc -l)

# The ceiling in base 10. The guard above accepts a digit string, so one with
# a leading zero is a number bash would read as octal: 08 is no octal digit and
# $(( )) would fail on it. The room and the over figures are the first places
# the ceiling meets arithmetic, so the base is forced here, once.
max=$((10#"$max"))

# The lines a path counts for at a revision, 0 if it is not counted there.
rev_lines() { # rev path
  local first
  case $2 in bin/* | lib/*.sh | libexec/*) ;; *) echo 0 && return ;; esac
  first=$(git show "$1:$2" 2>/dev/null | head -n 1)
  case $first in *bash*) git show "$1:$2" | wc -l ;; *) echo 0 ;; esac
}
base_lines() { rev_lines "$base" "$1"; }
now_lines() { # path
  local f
  for f in ${files[@]+"${files[@]}"}; do
    [ "$f" = "$1" ] && { [ -f "$1" ] && wc -l <"$1" || echo 0; return; }
  done
  echo 0
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

# The change's own counted paths: the two figures are the base's total and
# these paths' part of it, so nothing else is read.
changed=()
if [ -n "$base" ]; then
  # Paths are read with -z: the default separator makes git print a quoted
  # name for a path that holds a space, which would name no file on disk.
  mapfile -d '' -t changed < <(
    {
      git diff --name-only -z "$base" -- bin lib libexec
      git ls-files --others --exclude-standard -z -- bin lib libexec
    } | sort -zu
  )
fi

# The base's count, the room under it, and what the change adds. A tree with
# work in it would otherwise read as if the base had that room. The change is
# read from the working tree, uncommitted and untracked files included: a
# cycle has its bash in hand before it commits any of it, and the figure must
# name the lines it is about to add. Work not yet committed is said so too.
base_lines_total=$lines base_room=$((max - lines)) delta=0 base_figure=""
if [ -n "$base" ]; then
  # The one pass over the change's paths sums both sides: the lines they hold
  # now and the lines they hold at the base. A path counted only in one of the
  # two, or no longer there, holds no lines on that side and moves the delta on
  # its own. A line always comes back, so both names are always set. A file
  # nobody touched is not read at all: it counts the same on both sides.
  read -r now_now base_now < <(
    for f in ${changed[@]+"${changed[@]}"}; do
      printf '%d %d\n' "$(now_lines "$f")" "$(base_lines "$f")"
    done | awk '{ a += $1; b += $2 } END { printf "%d %d\n", a + 0, b + 0 }'
  )
  delta=$((now_now - base_now))
  base_lines_total=$((lines - delta))
  base_room=$((max - base_lines_total))
fi

# Whether the working tree differs from HEAD, counted files or not: the figure
# is the tree's, and a tree with work in it must not read as the base. Status
# is read from porcelain rather than from a quiet diff, because the diff of a
# tree that differs can reach the output of the run.
dirty=''
if git rev-parse --git-dir >/dev/null 2>&1; then
  [ -z "$(git status --porcelain 2>/dev/null)" ] || dirty=1
fi

# The base's figure, on a line of its own so the first line of the run stays
# the working tree's figure: the ceiling is judged by the tree, and the base's
# figure is the room the change starts from. The first word of both lines
# stays check-bash-lines:, and a change that takes lines out keeps its minus.
if [ -n "$base" ]; then
  if [ "$delta" -ge 0 ]; then
    base_figure="base $base_lines_total of $max: $base_room lines of room; this change +$delta"
  else
    base_figure="base $base_lines_total of $max: $base_room lines of room; this change $delta"
  fi
fi

if [ "$lines" -le "$max" ]; then
  line="check-bash-lines: $lines of $max: $((max - lines)) lines of room"
  [ -z "$base_figure" ] || line+="; $base_figure"
  [ -z "$dirty" ] || line+=" (working tree: includes uncommitted work)"
  echo "$line"
  exit 0
fi

echo "check-bash-lines: $lines lines of bash, over the ceiling of $max: $((lines - max)) lines over (scripts/bash-lines.max)."
[ -z "$base_figure" ] || echo "check-bash-lines: $base_figure"
[ -z "$dirty" ] || echo "The working tree differs from HEAD: the figure includes uncommitted work."
echo "Take out $((lines - max)) lines."
echo "A raise of the ceiling says why this cannot go into Rust now, and which issue takes the lines out again."

shown=0
if [ -n "$base" ]; then
  report=$(
    for f in ${changed[@]+"${changed[@]}"}; do
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

  # Where the lines can come from: the five largest counted files the change
  # did not touch. A file the change grew is where the lines went in, not
  # where they come out, so it is not on this list. The names come from the
  # count itself and the line holds a tab between the figure and the path, so
  # a name with spaces in it stays one path.
  if [ "${#files[@]}" -gt 0 ]; then
    pay=$(
      printf '%s\n' "${files[@]}" | while IFS= read -r f; do
        for c in ${changed[@]+"${changed[@]}"}; do
          [ "$c" = "$f" ] && continue 2
        done
        printf '%d\t%s\n' "$(wc -l <"$f")" "$f"
      done | sort -nr | head -n 5
    )
    if [ -n "$pay" ]; then
      echo "Pay from: the five largest counted files the change did not touch."
      while IFS=$'\t' read -r n f; do
        printf '  %6d  %s\n' "$n" "$f"
      done <<EOF
$pay
EOF
    fi
  fi
fi

if [ "$shown" = 0 ]; then
  echo "Largest files:"
  wc -l "${files[@]}" | grep -v ' total$' | sort -nr | head -n 5 | awk '{ printf "  %6d  %s\n", $1, $2 }'
fi
echo "Rule: take out as many lines as you add. Do not raise the ceiling; put new logic in the Rust workspace under console/."
exit 1
