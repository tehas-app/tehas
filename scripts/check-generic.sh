#!/usr/bin/env bash
# Lint: nothing here may give away one installation.
#
# Every commit to this repository is public and nothing filters a push. This
# keeps two things out:
#   - private network addresses, in every file: the private IPv4 ranges, the
#     tailnet range, link-local, IPv6 unique local addresses and tailnet names
#   - the names listed in scripts/check-generic.names, when that file is there,
#     in the files every instance installs. The list holds names that belong
#     to particular installations. It is never committed: .gitignore names it.
#
# The list of names is read as data, never run. One entry a line:
#     name                  a word to refuse (an extended regular expression)
#     allow <path prefix>   files that may use the names
#     # comment
# It cannot change the address rule or exempt a file from it.
#
# It is a guard against accidents, not evidence that the platform is generic:
# that evidence is an instance completing a cycle for another product. It
# reads text; an address inside an image or an archive is not seen.
#
# Examples in the manual and in command help use the instance that works on
# tehas itself (host tehas-tehas); those names are allowed. Addresses in
# examples come from the documentation ranges (192.0.2.0/24 and the like).
#
# Usage: scripts/check-generic.sh      exit 0 clean, 1 with the offending lines
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

# A private address or name. An IPv4 address counts only when it does not
# continue a longer number.
v4='(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01])|169\.254|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}'
addresses="(^|[^0-9.])$v4([^0-9]|\$)|(^|[^0-9a-f:])f[cd][0-9a-f]{2}(:[0-9a-f]{0,4}){2,}|[a-z0-9-]+\.ts\.net([^a-z0-9-]|\$)"

# Names, and the files that may use them. Prefixes of repository paths.
names=""
exceptions=(
  "docs/" # plans, specs, decisions and logs: the record of what happened
)
if [ -r scripts/check-generic.names ]; then
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      '' | '#'*) ;;
      'allow '*) exceptions+=("${line#allow }") ;;
      *[[:space:]=\"\'\$\`\;]*)
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

hits=0
report() { # <path> <matching lines>
  printf '%s\n' "$2" | sed "s|^|$1:|"
  hits=$((hits + $(printf '%s\n' "$2" | wc -l)))
}
while IFS= read -r path; do
  [ -f "$path" ] || continue
  [ "$path" = scripts/check-generic.names ] && continue
  if out=$(grep -n -a -i -E "$addresses" "$path"); then report "$path" "$out"; fi
  [ -n "$names" ] || continue
  excluded "$path" && continue
  if out=$(grep -n -I -i -E "$names" "$path"); then report "$path" "$out"; fi
done < <(git ls-files --cached --others --exclude-standard)

if [ "$hits" -gt 0 ]; then
  echo "check-generic: $hits lines name a particular installation or a private address (see $0)" >&2
  exit 1
fi
echo "check-generic: ok"
