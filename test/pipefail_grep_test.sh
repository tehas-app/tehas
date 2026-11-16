#!/usr/bin/env bash
# Tests for the shell tests themselves: no test pipes into grep -q. lib.sh sets
# pipefail, grep -q exits at its first match, and the writer still writing gets
# SIGPIPE. The pipeline then returns 141 and the case fails though the line is
# there (#2307). A here-string, a process substitution or a grep without -q
# reads the input whole.
# shellcheck disable=SC2016  # the awk program is literal
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# piped_grep_q <file>...: each line that pipes into grep -q, as file:line. A
# pipe that ends one line and a grep -q that opens the next count as one; an
# or-list (||) is no pipe.
piped_grep_q() {
  awk '
    FNR == 1 { prev = "" }
    /(^|[^|])[|][ \t]*grep[ \t]+-[A-Za-z]*q/ { print FILENAME ":" FNR }
    prev ~ /(^|[^|])[|][ \t]*\\?[ \t]*$/ && /^[ \t]*grep[ \t]+-[A-Za-z]*q/ { print FILENAME ":" FNR }
    { prev = $0 }
  ' "$@"
}

t_case "the check finds a pipe into grep -q, on one line or two"
cat >"$T/sample.sh" <<'EOF'
printf '%s\n' "$a" | grep -qxF -- "$b" || t_bad one
if [ -n "$a" ] && printf '%s\n' "$a" |
  grep -qF "$b"; then :; fi
grep -qxF -- "$b" <<<"$a" || t_bad here-string
grep -q "$b" < <(printf '%s\n' "$a") || t_bad process-substitution
printf '%s\n' "$a" | grep -c "$b"
[ -n "$a" ] || grep -q "$b" "$f" ||
  grep -q "$b" "$g"
EOF
t_eq "$(piped_grep_q "$T/sample.sh")" "$T/sample.sh:1
$T/sample.sh:3"

# The tests of improve go with the improve loop, in a change of their own.
t_case "no shell test pipes into grep -q"
files=()
for f in "$T_ROOT"/test/*.sh; do
  case $(basename "$f") in improve_* | pipefail_grep_test.sh) continue ;; esac
  files+=("$f")
done
found=$(piped_grep_q "${files[@]}")
[ -z "$found" ] || t_bad "a pipe into grep -q, use a here-string: ${found//$T_ROOT\//}"

t_done
