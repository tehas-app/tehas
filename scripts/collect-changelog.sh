#!/usr/bin/env bash
# Move the changelog fragments into a section of CHANGELOG.md.
#
# A pull request adds its lines in a file of its own, changelog.d/<issue>.md
# beside CHANGELOG.md (named after the branch when there is no issue), so two
# open pull requests do not conflict in CHANGELOG.md. A fragment has up to two
# sections, "### Changes" and "### Adopter action", each holding the lines it
# adds; text outside them is refused. The fragments are taken in the order of
# their issue numbers, then the named ones by name.
#
# The lines of each fragment's Changes go at the end of the section's
# "### Changes", and each fragment's Adopter action is a paragraph of its own
# at the end of the section's "### Adopter action", in place of a lone "None.".
# A heading the section lacks is made.
#
# Usage: scripts/collect-changelog.sh [--print] [--section <name>] [<file>]
#   --print     print the changelog with the fragments' lines in it on stdout;
#               change no file
#   --section   the release section to collect into, default Unreleased
#   <file>      the changelog, default the CHANGELOG.md beside scripts/; the
#               fragments are in changelog.d/ beside it
#
# Without --print the changelog is rewritten, the fragments are deleted, and
# each deleted fragment's path is a line on stdout. With no fragments nothing
# changes.
#
# Example:
#   scripts/collect-changelog.sh --section v0.25.0
#
# Exit: 0 collected, or printed; 1 the changelog cannot be read or written, it
#       has no such section, or a fragment has text outside its two headings;
#       64 a bad argument
set -uo pipefail

print=""
section=Unreleased
file=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --print) print=1 ;;
    --section)
      [ "$#" -ge 2 ] && [ -n "$2" ] || { echo "collect-changelog: --section needs a name" >&2; exit 64; }
      section=$2
      shift
      ;;
    -h | --help) sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; exit 0 ;;
    -*) echo "collect-changelog: unknown argument: $1" >&2; exit 64 ;;
    *)
      [ -z "$file" ] || { echo "collect-changelog: one changelog only" >&2; exit 64; }
      file=$1
      ;;
  esac
  shift
done
file=${file:-$(dirname "${BASH_SOURCE[0]}")/../CHANGELOG.md}
dir=$(dirname "$file")/changelog.d
text=$(cat "$file" 2>/dev/null) || { echo "collect-changelog: cannot read $file" >&2; exit 1; }

# Issue numbers first, in their order, then the names a branch gave.
fragments=()
if [ -d "$dir" ]; then
  mapfile -t fragments < <(
    cd "$dir" && for f in *.md; do [ -f "$f" ] && printf '%s\n' "$f"; done |
      LC_ALL=C sort -t . -k 1,1n -k 1,1 | awk '/^[0-9]+\.md$/ { print; next } { rest[++n] = $0 } END { for (i = 1; i <= n; i++) print rest[i] }'
  )
fi
if [ "${#fragments[@]}" = 0 ]; then
  [ -z "$print" ] || printf '%s\n' "$text"
  exit 0
fi

# The lines under one heading of a fragment, without blank lines at its ends.
block() { # <fragment> <heading>
  awk -v want="$2" '
    /^#/ {
      if ($0 ~ /^### Changes[[:space:]]*$/) at = "Changes"
      else if ($0 ~ /^### Adopter action[[:space:]]*$/) at = "Adopter action"
      else { bad = NR; exit }
      next
    }
    at == "" && NF { bad = NR; exit }
    at == want { l[++n] = $0; if (NF) { if (!first) first = n; last = n } }
    END {
      if (bad) { print "collect-changelog: " FILENAME " line " bad ": text outside \"### Changes\" and \"### Adopter action\"" > "/dev/stderr"; exit 1 }
      for (i = first; first && i <= last; i++) print l[i]
    }
  ' "$1"
}

changes="" action=""
for f in "${fragments[@]}"; do
  c=$(block "$dir/$f" Changes) || exit 1
  a=$(block "$dir/$f" "Adopter action") || exit 1
  [ -z "$c" ] || changes+=${changes:+$'\n'}$c
  [ -z "$a" ] || action+=${action:+$'\n\n'}$a
done

merged=$(COLLECT_CHANGES=$changes COLLECT_ACTION=$action awk -v name="$section" '
  { L[++n] = $0 }
  function add(i, t) {
    if (i in ins) ins[i] = ins[i] "\n" t
    else ins[i] = t
  }
  # The last line with text in the subsection that starts at h, or h.
  function last(h, i, l) {
    l = h
    for (i = h + 1; i < e && L[i] !~ /^### /; i++) if (L[i] ~ /[^[:space:]]/) l = i
    return l
  }
  END {
    for (i = 1; i <= n; i++) if (L[i] ~ /^## /) {
      if (s) { e = i; break }
      if (substr(L[i], 4, length(name)) == name && (length(L[i]) == 3 + length(name) || substr(L[i], 4 + length(name), 1) == " ")) s = i
    }
    if (!s) exit 3
    if (!e) e = n + 1
    end = s
    for (i = s + 1; i < e; i++) {
      if (L[i] ~ /^### Changes[[:space:]]*$/) ch = i
      if (L[i] ~ /^### Adopter action[[:space:]]*$/) ad = i
      if (L[i] ~ /^### / && !sub1) sub1 = i
      if (L[i] ~ /[^[:space:]]/) end = i
    }
    C = ENVIRON["COLLECT_CHANGES"]; A = ENVIRON["COLLECT_ACTION"]
    if (C != "") {
      if (ch) { l = last(ch); add(l, (l == ch ? "\n" : "") C) }
      else if (sub1) pre[sub1] = "### Changes\n\n" C "\n"
      else add(end, "\n### Changes\n\n" C)
    }
    if (A != "") {
      if (ad) {
        l = last(ad); none = 0
        for (i = ad + 1; i <= l; i++) if (L[i] ~ /[^[:space:]]/) none += (L[i] ~ /^None\.[[:space:]]*$/) ? 1 : 2
        if (none == 1) { drop[l] = 1; add(l, A) }
        else add(l, "\n" A)
      } else add(end, "\n### Adopter action\n\n" A)
    }
    for (i = 1; i <= n; i++) {
      if (i in pre) print pre[i]
      if (!(i in drop)) print L[i]
      if (i in ins) print ins[i]
    }
  }
' <<<"$text")
case $? in
  0) ;;
  3) echo "collect-changelog: no section for $section in $file" >&2; exit 1 ;;
  *) exit 1 ;;
esac

if [ -n "$print" ]; then
  printf '%s\n' "$merged"
  exit 0
fi
if ! { printf '%s\n' "$merged" >"$file.collect" && mv "$file.collect" "$file"; }; then
  rm -f "$file.collect"
  echo "collect-changelog: cannot write $file" >&2
  exit 1
fi
for f in "${fragments[@]}"; do
  rm -f "$dir/$f" || { echo "collect-changelog: cannot delete $dir/$f" >&2; exit 1; }
  printf '%s\n' "$dir/$f"
done
rmdir "$dir" 2>/dev/null || true
