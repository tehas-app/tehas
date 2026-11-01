# shellcheck shell=bash
# The millwright's account and the person's approval a record rests on, and
# the marker of a park. Sourced by libexec/cycle and libexec/work. Needs
# lib/labels.sh, and lib/store.sh for the history a park repeats.
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
#   tehas_park_marker <issue> <step> <cap> <park reason> <failure kind>
#                     <failure cause> <provider 0|1>
#                                 the marker line of a park comment, for a
#                                 fixer that reads GitHub: step and kind as the
#                                 history line has them, cut to [a-z0-9-], and
#                                 sig, the sha256 of the history line's
#                                 step|cap|park_reason|failure_kind|failure_cause
#   tehas_park_repeat <issue> <marker>
#                                 the word 1 when the end the issue's history
#                                 lists last is a park of <marker>'s sig and no
#                                 provider outage, so this park repeats it, and
#                                 nothing otherwise, and 64 when <marker> is not
#                                 one of this issue. Only that one end counts, so
#                                 a merge, a handoff or any end that was not a
#                                 park breaks the run, and an outage park counts
#                                 for nothing. The sig is compared, never the
#                                 free text of a failure.
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

# The free text is in the sig only, so nothing of it reaches the marker.
tehas_park_marker() { # <issue> <step> <cap> <park reason> <failure kind> <failure cause> <provider>
  local LC_ALL=C step kind sig provider=0
  [[ $1 =~ ^[1-9][0-9]*$ ]] || return 1
  step=$2
  [ "$step" != - ] || step=""
  step=${step//[!a-z0-9-]/} kind=${5//[!a-z0-9-]/}
  step=${step:0:40} kind=${kind:0:40}
  [ "$7" != 1 ] || provider=1
  sig=$(printf '%s|%s|%s|%s|%s' "$2" "$3" "$4" "$5" "$6" | sha256sum | cut -c1-64) || return 1
  printf '<!-- tehas:park issue=%s step=%s kind=%s provider=%s sig=%s -->\n' "$1" "$step" "$kind" "$provider" "$sig"
}

# tehas_park_repeat <issue> <marker>: prints 1 when the last end of the issue in
# its history is a park of <marker>'s sig and no provider outage, so this park
# repeats it, and nothing otherwise: a merge, a handoff or any end that was not a
# park breaks the run, and an outage park counts for nothing. Nothing when the
# history cannot be read; 64 when <marker> is not one of this issue.
tehas_park_repeat() { # <issue> <marker>
  local marker=$2 step sig fields re
  # The pattern is held in a variable: bash takes the word between the angle
  # brackets of a literal pattern for a command word.
  re='^<!-- tehas:park issue=([1-9][0-9]*) step=([a-z0-9-]{0,40}) kind=[a-z0-9-]{0,40} provider=([01]) sig=([0-9a-f]{64}) -->$'
  [[ $marker =~ $re && ${BASH_REMATCH[1]} = "$1" ]] || return 64
  [ "${BASH_REMATCH[3]}" != 1 ] || return 0
  step=${BASH_REMATCH[2]} sig=${BASH_REMATCH[4]}
  # The park is recorded after this, and the store keeps an answer only until
  # its next call.
  tehas_store history-show "$1" || return 0
  # The five words of the park, joined, from the program of the file: equal
  # hashes, equal parks, and no free text of a failure in this shell.
  # shellcheck disable=SC2154  # lib/store.sh, which the caller sources, sets this
  fields=$(jq -r --arg step "$step" -f "${BASH_SOURCE[0]%/*}/park-fields.jq" <<<"$tehas_store_out" 2>/dev/null) || return 0
  [ -n "$fields" ] && [ "$sig" = "$(printf '%s' "$fields" | sha256sum | cut -c1-64)" ] || return 0
  printf '1\n'
}
