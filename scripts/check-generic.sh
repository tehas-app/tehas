#!/usr/bin/env bash
# Lint: nothing here may give away one installation.
#
# Every commit to this repository is public and nothing filters a push. This
# keeps two things out:
#   - private network addresses, in every file: the private IPv4 ranges, the
#     tailnet range, link-local, IPv6 unique local addresses and tailnet names
#   - the names listed in scripts/check-generic.names, when that file is there,
#     in every file, docs/ included: the record is public too. The list holds
#     names that belong to particular installations. It is never committed:
#     .gitignore names it. Without it the lint says so and checks addresses
#     only.
#
# The list of names is read as data, never run. One entry a line:
#     name                  a word to refuse (an extended regular expression)
#     allow <path prefix>   files that may use the names; none by default
#     # comment
# It cannot change the address rule or exempt a file from it.
#
# It is a guard against accidents, not evidence that the platform is generic:
# that evidence is an instance completing a cycle for another product. It
# reads text; an address inside an image or an archive is not seen.
#
# With --messages <rev-range> it also checks the names (not the addresses) in
# the commit messages of the range and in the branch name, taken from
# GITHUB_HEAD_REF or else the current branch. Run it on a pull request with
# the range from the base branch to HEAD.
#
# Examples in the manual and in command help use the instance that works on
# tehas itself (host tehas-tehas); those names are allowed. Addresses in
# examples come from the documentation ranges (192.0.2.0/24 and the like).
#
# File paths are checked against the names too, tracked or not and ignored
# files aside: a path that matches is reported as "<path>: path names a listed
# name". An allow line exempts a path from this as from the content check.
#
# With --names-only the address rule is skipped, for an infrastructure
# repository that must hold addresses. Names in content, paths and, with
# --messages, commit messages and the branch are still checked. Without a names
# list it says so and exits 0. The flags combine in either order.
#
# Usage: scripts/check-generic.sh [--names-only] [--messages <rev-range>]
# Exit 0 clean, 1 with the offending lines, 2 on a bad argument.
set -uo pipefail

usage="usage: scripts/check-generic.sh [--names-only] [--messages <rev-range>]"
range=""
names_only=0
while [ $# -gt 0 ]; do
  case "$1" in
    --names-only) names_only=1 ;;
    --messages)
      range=${2:-}
      if [ -z "$range" ] || [ "${range#-}" != "$range" ]; then
        echo "$usage" >&2
        exit 2
      fi
      shift
      ;;
    *)
      echo "$usage" >&2
      exit 2
      ;;
  esac
  shift
done

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

if [ -n "$range" ] && ! git rev-list --end-of-options "$range" >/dev/null 2>&1; then
  echo "check-generic: cannot read the commits in $range" >&2
  exit 2
fi

# A private address or name. An IPv4 address counts only when it does not
# continue a longer number.
v4='(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01])|169\.254|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}'
addresses="(^|[^0-9.])$v4([^0-9]|\$)|(^|[^0-9a-f:])f[cd][0-9a-f]{2}(:[0-9a-f]{0,4}){2,}|[a-z0-9-]+\.ts\.net([^a-z0-9-]|\$)"

# Names, and the files that may use them. Prefixes of repository paths.
names=""
exceptions=()
have_list=0
if [ -r scripts/check-generic.names ]; then
  have_list=1
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      '' | '#'*) ;;
      'allow '*) exceptions+=("${line#allow }") ;;
      *[[:space:]=\"\'\`\;]*)
        echo "check-generic: scripts/check-generic.names: not a name or an allow line: $line" >&2
        exit 1
        ;;
      *) names="${names:+$names|}$line" ;;
    esac
  done <scripts/check-generic.names
fi

excluded() {
  local path=$1 e
  for e in "${exceptions[@]}"; do
    [ -n "$e" ] || continue
    case "$path" in "$e"*) return 0 ;; esac
  done
  return 1
}

if [ "$have_list" -eq 0 ]; then
  echo "check-generic: no names list, names not checked"
  [ "$names_only" -eq 0 ] || exit 0
fi

hits=0
shown() { printf '%s' "${1//$'\n'/\\n}"; }
report() { # <path> <matching lines>
  printf '%s\n' "$2" | sed "s|^|$(shown "$1"):|"
  hits=$((hits + $(printf '%s\n' "$2" | wc -l)))
}
while IFS= read -r -d '' path; do
  [ -f "$path" ] || continue
  [ "$path" = scripts/check-generic.names ] && continue
  if [ "$names_only" -eq 0 ] && out=$(grep -n -a -i -E "$addresses" "$path"); then report "$path" "$out"; fi
  [ -n "$names" ] || continue
  excluded "$path" && continue
  printf '%s\n' "$path" | grep -q -i -E "$names"
  case $? in
    0)
      echo "$(shown "$path"): path names a listed name"
      hits=$((hits + 1))
      ;;
    1) ;;
    *) echo "check-generic: invalid name expression in scripts/check-generic.names" >&2; exit 1 ;;
  esac
  if out=$(grep -n -I -i -E "$names" "$path"); then
    report "$path" "$out"
  else
    rc=$?
    [ "$rc" -le 1 ] || { echo "check-generic: invalid name expression in scripts/check-generic.names" >&2; exit 1; }
  fi
done < <(git ls-files -z --cached --others --exclude-standard)

if [ -n "$range" ] && [ -n "$names" ]; then
  if ! log=$(git log --format=%B --end-of-options "$range"); then
    echo "check-generic: cannot read the commits in $range" >&2
    exit 2
  fi
  if out=$(printf '%s\n' "$log" | grep -n -i -E "$names"); then
    report "commit messages" "$out"
  fi
  branch=${GITHUB_HEAD_REF:-$(git symbolic-ref -q --short HEAD)}
  if printf '%s\n' "$branch" | grep -q -i -E "$names"; then
    echo "branch: $branch"
    hits=$((hits + 1))
  fi
fi

if [ "$hits" -gt 0 ]; then
  echo "check-generic: $hits lines name a particular installation or a private address (see $0)" >&2
  exit 1
fi
echo "check-generic: ok"
