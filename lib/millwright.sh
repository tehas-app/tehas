# shellcheck shell=bash
# The millwright's account and the person's approval a record rests on.
# No command sources it now: tehas work's rules are console/cycle/src/gives.rs.
# Needs lib/labels.sh.
#
# The millwright is an account of its own, TEHAS_MILLWRIGHT_LOGIN, that decides
# on parked issues from outside this factory: whatever runs as that account, the
# dispatcher's grant is what it may do (console/dispatch/src/decide.rs,
# fixer_grant). The approval record of a start says which person's
# approval a millwright start rests on (rests_on) and how many fixer starts there
# have been on it (fixer_starts).
#
#   tehas_millwright_on                0 when the instance has a millwright
#   tehas_millwright_is <login>        0 when <login> is the millwright's account
#   tehas_millwright_base <record>     the person's approval a record rests on, as
#                                 {at, by, how, body_sha256}: the record's own
#                                 when a person gave it, its rests_on when the
#                                 millwright did. Fails for a missing record, a link,
#                                 a shape it does not know, or an account that
#                                 is not allowed now. A refresh's rests_on too.
#
# Sourcing this file does nothing.

tehas_millwright_on() { [ -n "${TEHAS_MILLWRIGHT_LOGIN:-}" ]; }

tehas_millwright_is() { local l=${1:-}; tehas_millwright_on && [ "${l%\[bot\]}" = "${TEHAS_MILLWRIGHT_LOGIN%\[bot\]}" ]; }

tehas_millwright_base() { # <record>
  local base by
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  # shellcheck disable=SC2016  # a jq program
  base=$(jq -c 'def ok: type == "object" and (.at | type) == "string" and (.at | test("^[0-9T:.Z-]+\\z"))
      and (.by | type) == "string" and (.by | test("^[A-Za-z0-9-]+\\z"))
      and (.how | type) == "string" and (.how | test("^(label|retry-tick|outage-retry)\\z"))
      and (.body_sha256 | type) == "string" and (.body_sha256 | test("^[0-9a-f]{64}\\z"));
    (if .how == "fixer" or .how == "refresh" then .rests_on else . end) | {at, by, how, body_sha256}
    | if ok then . else error("not an approval") end' "$1" 2>/dev/null) || return 1
  by=$(jq -r .by <<<"$base") || return 1
  tehas_allowed_labeler "$by" || return 1
  printf '%s\n' "$base"
}
