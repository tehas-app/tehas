#!/usr/bin/env bash
# Lint: platform files must not name one particular product or host.
#
# Files that every instance installs have to work for any product. This keeps
# two things out of them:
#   - private network addresses (the tailnet range), always
#   - the names listed in scripts/check-generic.names, when that file is there.
#     It holds names that belong to particular installations and is not part
#     of the published repository; without it only addresses are checked.
#
# It is a guard against regressions, not evidence that the platform is generic:
# that evidence is an instance completing a cycle for another product.
#
# Examples in the manual and in command help use the instance that works on
# tehas itself (host tehas-tehas); those names are allowed.
#
# Usage: scripts/check-generic.sh      exit 0 clean, 1 with the offending lines
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

# An address in the tailnet range (the carrier-grade NAT block).
words='\b100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}\b'

# Files that may use them, each with the reason. Prefixes of repository paths.
exceptions=(
  "docs/"                       # plans, specs, decisions and logs: the record of what happened
)

# More words and exceptions, in a checkout that has them.
if [ -r scripts/check-generic.names ]; then
  # shellcheck source=/dev/null
  . scripts/check-generic.names
fi

excluded() {
  local path=$1 e
  for e in "${exceptions[@]}"; do
    case "$path" in "$e"*) return 0 ;; esac
  done
  return 1
}

hits=0
while IFS= read -r path; do
  excluded "$path" && continue
  [ -f "$path" ] || continue
  if out=$(grep -n -I -i -E "$words" "$path"); then
    printf '%s\n' "$out" | sed "s|^|$path:|"
    hits=$((hits + $(printf '%s\n' "$out" | wc -l)))
  fi
done < <(git ls-files --cached --others --exclude-standard)

if [ "$hits" -gt 0 ]; then
  echo "check-generic: $hits lines name a particular product or host (see the list of exceptions in $0)" >&2
  exit 1
fi
echo "check-generic: ok"
