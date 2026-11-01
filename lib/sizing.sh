# shellcheck shell=bash
# The measure of an issue body and its ledger, console/src/size.rs. Sourced.
tehas_size_console=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/libexec/console
tehas_size_why=""
tehas_issue_size() { "${TEHAS_CONSOLE_CMD:-$tehas_size_console}" size measure; }
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_size_record() { # <issue> <body> <ledger>; $tehas_size_why says which step gave up
  tehas_size_why=$("${TEHAS_CONSOLE_CMD:-$tehas_size_console}" size record "$1" "$3" <<<"${2:-}" 2>/dev/null) && return 0
  [ "$tehas_size_why" = ledger ] || tehas_size_why=measure
  return 1
}
