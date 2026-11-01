#!/usr/bin/env bash
# The polyfills: what tehas does itself for a pi-rukas feature that no release
# has yet, and their removal once one has it.
#
# A polyfill of the feature <f>, a key of the features in the runner's
# --capabilities answer, is three things:
#   lib/polyfill-<f>.sh          the code, with two header lines naming where
#                                pi-rukas adds the feature, so a polyfill stands
#                                in for work upstream has taken on and never for
#                                one nobody will build:
#                                `# Upstream issue: <owner>/<repo>#<n>`
#                                `# Upstream PR: <owner>/<repo>#<n>`
#   test/polyfill_<f>_test.sh    its test
#   lines that use it            anywhere else, each a line of its own ending in
#                                `# polyfill <f>`
# A caller asks tehas-cycle caps mode, which answers native
# where the image has the feature, so a polyfill is not used once the image has
# it. Removing one is deleting those files and lines, which is what prune does.
# scripts/tag-release.sh runs prune before it tags.
#
# Usage: scripts/polyfills.sh [--root <dir>] list
#        scripts/polyfills.sh [--root <dir>] check
#        scripts/polyfills.sh [--root <dir>] expired <caps file>
#        scripts/polyfills.sh [--root <dir>] prune <caps file>
#   list     one line per polyfill: the feature, its upstream issue and its
#            upstream pull request
#   check    each polyfill has both upstream lines and its test, and each marked
#            line names a polyfill: one line per problem
#   expired  the polyfills whose feature the --capabilities answer in
#            <caps file> sets to true, one per line
#   prune    remove the expired ones from the tree, then check that each shell
#            file it changed still parses. Prints each path it removed or changed
#   --root   the tree, default the checkout this script is in
#
# It reads the tree and the answer and nothing else: no network, no git. That
# the issue and the pull request exist, and that the pull request was not closed
# unmerged, is checked by scripts/tag-release.sh, which asks GitHub.
#
# Exit: 0 done, and for check no problem; 1 check found a problem, the answer
#       is not a JSON object, a file could not be changed, or a changed file no
#       longer parses; 64 a bad argument
set -uo pipefail

usage() { sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; }
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [ "${1:-}" = --root ]; then
  [ "$#" -ge 2 ] && [ -d "$2" ] || { echo "polyfills: --root needs a directory" >&2; exit 64; }
  root=$(cd "$2" && pwd)
  shift 2
fi
verb=${1:-}
case "$verb" in
  list | check) [ "$#" -eq 1 ] || { usage >&2; exit 64; } ;;
  expired | prune)
    [ "$#" -eq 2 ] && [ -f "$2" ] || { echo "polyfills: $verb needs a --capabilities answer file" >&2; exit 64; }
    caps=$2
    ;;
  -h | --help) usage; exit 0 ;;
  *) usage >&2; exit 64 ;;
esac

name_re='^[a-z][A-Za-z0-9]{0,63}$'
marker_re='# polyfill ([a-z][A-Za-z0-9]{0,63})[[:space:]]*$'

# The features tehas carries a polyfill of, one per line.
features() {
  local f n
  for f in "$root"/lib/polyfill-*.sh; do
    [ -f "$f" ] || continue
    n=${f##*/polyfill-}
    n=${n%.sh}
    [[ ! $n =~ $name_re ]] || printf '%s\n' "$n"
  done
}

upstream() { # <feature> <issue|PR>
  sed -n -E "s/^# Upstream $2: ([A-Za-z0-9_.-]+\\/[A-Za-z0-9_.-]+#[0-9]+)[[:space:]]*\$/\\1/p" \
    "$root/lib/polyfill-$1.sh" | head -n 1
}

# The files that may hold a marked line: the tree, less the records in docs/,
# the instances and what is built.
searched() {
  find "$root" \( -path "$root/.git" -o -path "$root/docs" -o -path "$root/instances" \
    -o -name target -o -name node_modules \) -prune -o -type f -print0 |
    xargs -0 grep -I -l -E -- "$marker_re" 2>/dev/null
}

marked() { # <feature>: the files with a line marked for it
  local f
  while IFS= read -r f; do
    ! grep -q -E -- "# polyfill ${1}[[:space:]]*\$" "$f" || printf '%s\n' "$f"
  done < <(searched)
}

expired() {
  local f
  jq -e 'type == "object"' "$caps" >/dev/null 2>&1 || { echo "polyfills: $caps is not a JSON object" >&2; return 1; }
  while IFS= read -r f; do
    ! jq -e --arg f "$f" '(.features | type) == "object" and .features[$f] == true' "$caps" >/dev/null 2>&1 ||
      printf '%s\n' "$f"
  done < <(features)
}

case "$verb" in
  list)
    while IFS= read -r f; do printf '%s %s %s\n' "$f" "$(upstream "$f" issue)" "$(upstream "$f" PR)"; done < <(features)
    ;;
  check)
    bad=0
    have=$(features)
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      [ -n "$(upstream "$f" issue)" ] || { echo "lib/polyfill-$f.sh names no upstream issue"; bad=1; }
      [ -n "$(upstream "$f" PR)" ] || { echo "lib/polyfill-$f.sh names no upstream pull request"; bad=1; }
      [ -f "$root/test/polyfill_${f}_test.sh" ] || { echo "lib/polyfill-$f.sh has no test/polyfill_${f}_test.sh"; bad=1; }
    done <<<"$have"
    while IFS= read -r file; do
      while IFS= read -r line; do
        [[ $line =~ $marker_re ]] || continue
        grep -q -x -F -- "${BASH_REMATCH[1]}" <<<"$have" && continue
        echo "${file#"$root"/} marks a line for ${BASH_REMATCH[1]}, which has no lib/polyfill-${BASH_REMATCH[1]}.sh"
        bad=1
      done <"$file"
    done < <(searched)
    exit "$bad"
    ;;
  expired)
    expired || exit 1
    ;;
  prune)
    gone=$(expired) || exit 1
    changed=()
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      while IFS= read -r file; do
        sed -i -E "/# polyfill ${f}[[:space:]]*\$/d" "$file" || { echo "polyfills: cannot change $file" >&2; exit 1; }
        changed+=("$file")
        echo "${file#"$root"/}"
      done < <(marked "$f")
      for file in "$root/lib/polyfill-$f.sh" "$root/test/polyfill_${f}_test.sh"; do
        [ -e "$file" ] || continue
        rm -f "$file" || { echo "polyfills: cannot remove $file" >&2; exit 1; }
        echo "${file#"$root"/}"
      done
    done <<<"$gone"
    # A marked line that was part of a larger statement leaves a file that no
    # longer parses. That is said here rather than found by a cycle later.
    rc=0
    for file in "${changed[@]}"; do
      case "$(head -n 1 "$file")" in
        *bash*) ;;
        *) case "$file" in *.sh) ;; *) continue ;; esac ;;
      esac
      bash -n "$file" 2>/dev/null || { echo "polyfills: ${file#"$root"/} no longer parses" >&2; rc=1; }
    done
    exit "$rc"
    ;;
esac
