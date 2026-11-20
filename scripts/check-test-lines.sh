#!/usr/bin/env bash
# The ceiling on the shell of test/: the lines of the shell scripts under
# test/, subdirectories included, may not grow past the number in
# scripts/test-lines.max. A behaviour of Rust code is tested in Rust
# (AGENTS.md, rule 6); a shell test is for what is bash. A change that takes
# shell tests out lowers the number to the new count in the same pull
# request. A raise is a person's call.
#
# Usage: scripts/check-test-lines.sh
# Exit: 0 at or under the ceiling, 1 over it or the ceiling is unreadable
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

max=$(tr -d '[:space:]' <scripts/test-lines.max 2>/dev/null)
if ! [[ $max =~ ^[0-9]{1,18}$ ]]; then
  echo "check-test-lines: scripts/test-lines.max holds no number" >&2
  exit 1
fi
max=$((10#"$max"))

# A shell script as check-bash-lines.sh reads one: its shebang, or a
# directive for shellcheck's shell, names sh or bash.
is_shell() {
  awk 'NR == 1 && /^#!/ && /[\/ \t](ba)?sh([ \t]|$)/ { s = 1 }
    /^#[ \t]*shellcheck[ \t]+shell=(ba)?sh([ \t]|$)/ { s = 1 }
    END { exit !s }'
}

files=()
while IFS= read -r -d '' f; do
  is_shell <"$f" && files+=("$f")
done < <(find test -type f -print0 2>/dev/null | sort -z)
lines=0
[ "${#files[@]}" = 0 ] || lines=$(cat "${files[@]}" | wc -l)

if [ "$lines" -le "$max" ]; then
  echo "check-test-lines: $lines of $max: $((max - lines)) lines of room"
  [ "$lines" = "$max" ] || echo "check-test-lines: lower scripts/test-lines.max to $lines"
  exit 0
fi
echo "check-test-lines: $lines lines of shell in test/, over the ceiling of $max: $((lines - max)) lines over (scripts/test-lines.max)."
echo "Test a behaviour of Rust code in Rust, beside the source or in the crate's tests/ (AGENTS.md, rule 6)."
echo "Largest files:"
wc -l "${files[@]}" | grep -v ' total$' | sort -nr | head -n 5 | awk '{ printf "  %6d  %s\n", $1, $2 }'
exit 1
