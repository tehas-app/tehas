# shellcheck shell=bash
# The millwright's grant, as the dispatcher reads it, and the history of a
# cycle's ends. Sourced by libexec/dispatch, libexec/cycle and libexec/work.
# Needs lib/labels.sh, and lib/store.sh for the grant and the history.
#
# The millwright is an account of its own, TEHAS_MILLWRIGHT_LOGIN, that decides
# on parked issues from outside this factory: whatever runs as that account, the
# grant here is what it may do. A tehas:ready it
# applies counts only on an issue a person approved before, whose body is
# still the body that person approved, and only TEHAS_MILLWRIGHT_MAX_RETRIES times
# since that approval. The approval record of a start says which person's
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
#   tehas_millwright_starts <record>   the millwright starts the record counts: its
#                                 fixer_starts when the millwright gave it or a
#                                 refresh carried them on, else 0
#   tehas_millwright_grant <issue> <body sha256>
#                                 0 when a tehas:ready of the millwright's on the
#                                 issue may start a cycle now, by the approval
#                                 of its latest start on the state store
#                                 (tehas_approval_view). Sets
#                                 $tehas_millwright_rests_on (JSON) and
#                                 $tehas_millwright_count (the starts with this
#                                 one). On 1, $tehas_millwright_why says why not;
#                                 2 when the store could not answer, which
#                                 decides nothing.
#   tehas_millwright_answer <session>  0 when the millwright may answer the open
#                                 question of the session, a file as plan_view
#                                 writes it (lib/store.sh, lib/planning.sh): a
#                                 person allowed now started it, the millwright
#                                 gave fewer
#                                 than TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS of its
#                                 answers and not the last one. On 1,
#                                 $tehas_millwright_why says why not. What the
#                                 answer is, the caller checks.
#   tehas_cycle_history <issue>   the issue's cycle history on the state store
#                                 (history-show), one JSON object per line in
#                                 the shape the lines had before the store
#                                 (lib/history-lines.jq, the cycle rows).
#                                 Returns 1, printing nothing, when the store
#                                 could not answer
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

tehas_millwright_is() { tehas_millwright_on && [ "${1:-}" = "$TEHAS_MILLWRIGHT_LOGIN" ]; }

tehas_millwright_base() { # <record>
  local base by
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  # shellcheck disable=SC2016  # a jq program
  base=$(jq -c 'def ok: type == "object" and (.at | type) == "string" and (.at | test("^[0-9T:.Z-]+$"))
      and (.by | type) == "string" and (.by | test("^[A-Za-z0-9-]+$"))
      and (.how | type) == "string" and (.how | test("^(label|retry-tick|outage-retry)$"))
      and (.body_sha256 | type) == "string" and (.body_sha256 | test("^[0-9a-f]{64}$"));
    (if .how == "fixer" or .how == "refresh" then .rests_on else . end) | {at, by, how, body_sha256}
    | if ok then . else error("not an approval") end' "$1" 2>/dev/null) || return 1
  by=$(jq -r .by <<<"$base") || return 1
  tehas_allowed_labeler "$by" || return 1
  printf '%s\n' "$base"
}

tehas_millwright_starts() { # <record>
  jq -r 'if .how == "fixer" then (.fixer_starts | if type == "number" and . >= 1 and . == floor then . else error("no count") end)
    elif .how == "refresh" then (.fixer_starts // 0 | if type == "number" and . >= 0 and . == floor then . else error("no count") end)
    else 0 end' "$1" 2>/dev/null
}

# shellcheck disable=SC2034  # the caller reads what tehas_millwright_grant sets
tehas_millwright_rests_on=""
tehas_millwright_count=0
tehas_millwright_why=""
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_millwright_grant() { # <issue> <body sha256>
  local record="" had rc
  tehas_millwright_rests_on=""
  tehas_millwright_count=0
  tehas_millwright_why=""
  if [ "$1" = "${TEHAS_CONTROL_ISSUE:-}" ]; then
    tehas_millwright_why="it is the control issue"
    return 1
  fi
  record=$(tehas_approval_view "$1")
  rc=$?
  if [ "$rc" = 2 ]; then
    tehas_millwright_why="its approvals on the state store could not be read"
    return 2
  fi
  if [ "$rc" != 0 ] || ! tehas_millwright_rests_on=$(tehas_millwright_base "$record"); then
    rm -f "$record"
    tehas_millwright_rests_on=""
    tehas_millwright_why="no person approved an earlier cycle of it, so the fixer may not start one"
    return 1
  fi
  had=$(tehas_millwright_starts "$record") || had=""
  rm -f "$record"
  if [ "$2" != "$(jq -r .body_sha256 <<<"$tehas_millwright_rests_on")" ]; then
    tehas_millwright_why="its text is not the text $(jq -r .by <<<"$tehas_millwright_rests_on") approved"
    return 1
  fi
  if ! [[ $had =~ ^[0-9]{1,6}$ ]]; then
    tehas_millwright_why="its approval record does not say how many cycles the fixer started"
    return 1
  fi
  if [ "$had" -ge "${TEHAS_MILLWRIGHT_MAX_RETRIES:-2}" ]; then
    tehas_millwright_why="the fixer has started $had cycles on it since $(jq -r .by <<<"$tehas_millwright_rests_on") approved it, the limit"
    return 1
  fi
  tehas_millwright_count=$((had + 1))
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_millwright_answer() { # <session, as plan_view writes it>
  local by got had last
  tehas_millwright_why=""
  if ! tehas_millwright_on; then
    tehas_millwright_why="there is no fixer"
    return 1
  fi
  if [ ! -f "$1" ] || [ -L "$1" ]; then
    tehas_millwright_why="the session has no record on this host"
    return 1
  fi
  by=$(jq -r '.by | if type == "string" then . else "" end' "$1" 2>/dev/null) || by=""
  if [ -z "$by" ] || ! tehas_allowed_labeler "$by"; then
    tehas_millwright_why="no person's tehas:plan started this session, as far as its record says"
    return 1
  fi
  got=$(jq -r '.answers // [] | if type == "array" and all(.[]; type == "object" and (.fixer | type) == "boolean")
      then "\(map(select(.fixer)) | length) \(last.fixer // false)" else error("no list") end' "$1" 2>/dev/null) || got=""
  if ! [[ $got =~ ^([0-9]{1,6})\ (true|false)$ ]]; then
    tehas_millwright_why="its record does not say who answered its turns"
    return 1
  fi
  had=${BASH_REMATCH[1]}
  last=${BASH_REMATCH[2]}
  if [ "$last" = true ]; then
    tehas_millwright_why="the fixer answered the question before this one; a person answers this one"
    return 1
  fi
  if [ "$had" -ge "${TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS:-3}" ]; then
    tehas_millwright_why="the fixer has answered $had questions in this session, the limit"
    return 1
  fi
}

tehas_cycle_history() { # <issue>
  tehas_store history-show "$1" --kind cycle || return 1
  # shellcheck disable=SC2154  # lib/store.sh, which the caller sources, sets this
  jq -c --arg repo "${TEHAS_REPO:-}" --argjson kinds '["cycle"]' -f "${BASH_SOURCE[0]%/*}/history-lines.jq" <<<"$tehas_store_out" 2>/dev/null
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
  # The read is made here, not by tehas_cycle_history: the park is recorded after
  # this, and the store keeps an answer only until its next call.
  tehas_store history-show "$1" || return 0
  # The five words of the park, joined, from the program of the file: equal
  # hashes, equal parks, and no free text of a failure in this shell.
  fields=$(jq -r --arg step "$step" -f "${BASH_SOURCE[0]%/*}/park-fields.jq" <<<"$tehas_store_out" 2>/dev/null) || return 0
  [ -n "$fields" ] && [ "$sig" = "$(printf '%s' "$fields" | sha256sum | cut -c1-64)" ] || return 0
  printf '1\n'
}
