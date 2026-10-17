# shellcheck shell=bash
# The fixer's grant, as the dispatcher reads it, and the history of a cycle's
# ends a fixer decides from. Sourced by libexec/dispatch, libexec/cycle and
# libexec/work. Needs lib/labels.sh.
#
# The fixer is an account of its own, TEHAS_FIXER_LOGIN, that decides on
# parked issues from outside this factory: whatever runs as that account, the
# grant here is what it may do. A tehas:ready it
# applies counts only on an issue a person approved before, whose body is
# still the body that person approved, and only TEHAS_FIXER_MAX_RETRIES times
# since that approval. The approval record of a start says which person's
# approval a fixer start rests on (rests_on) and how many fixer starts there
# have been on it (fixer_starts).
#
#   tehas_fixer_on                0 when the instance has a fixer
#   tehas_fixer_is <login>        0 when <login> is the fixer's account
#   tehas_fixer_base <record>     the person's approval a record rests on, as
#                                 {at, by, how, body_sha256}: the record's own
#                                 when a person gave it, its rests_on when the
#                                 fixer did. Fails for a missing record, a link,
#                                 a shape it does not know, or an account that
#                                 is not allowed now.
#   tehas_fixer_starts <record>   the fixer starts the record counts: its
#                                 fixer_starts when the fixer gave it, else 0
#   tehas_fixer_grant <issue> <body sha256>
#                                 0 when a tehas:ready of the fixer's on the
#                                 issue may start a cycle now. Sets
#                                 $tehas_fixer_rests_on (JSON) and
#                                 $tehas_fixer_count (the starts with this
#                                 one). On 1, $tehas_fixer_why says why not.
#   tehas_cycle_history_add <issue> <JSON object>
#                                 append one line to
#                                 state/cycles/<issue>.history.jsonl, keeping
#                                 the last 50
#   tehas_cycle_history <issue>   that file's lines that are JSON objects
#   tehas_park_signature          print a jq definition, `park_signature`:
#                                 what makes two parks the same park
#
# Sourcing this file does nothing.

tehas_fixer_on() { [ -n "${TEHAS_FIXER_LOGIN:-}" ]; }

tehas_fixer_is() { tehas_fixer_on && [ "${1:-}" = "$TEHAS_FIXER_LOGIN" ]; }

tehas_fixer_base() { # <record>
  local base by
  [ -f "$1" ] && [ ! -L "$1" ] || return 1
  # shellcheck disable=SC2016  # a jq program
  base=$(jq -c 'def ok: type == "object" and (.at | type) == "string" and (.at | test("^[0-9T:.Z-]+$"))
      and (.by | type) == "string" and (.by | test("^[A-Za-z0-9-]+$"))
      and (.how | type) == "string" and (.how | test("^(label|retry-tick|outage-retry)$"))
      and (.body_sha256 | type) == "string" and (.body_sha256 | test("^[0-9a-f]{64}$"));
    (if .how == "fixer" then .rests_on else . end) | {at, by, how, body_sha256}
    | if ok then . else error("not an approval") end' "$1" 2>/dev/null) || return 1
  by=$(jq -r .by <<<"$base") || return 1
  tehas_allowed_labeler "$by" || return 1
  printf '%s\n' "$base"
}

tehas_fixer_starts() { # <record>
  jq -r 'if .how == "fixer" then (.fixer_starts | if type == "number" and . >= 1 and . == floor then . else error("no count") end)
    else 0 end' "$1" 2>/dev/null
}

# shellcheck disable=SC2034  # the caller reads what tehas_fixer_grant sets
tehas_fixer_rests_on=""
tehas_fixer_count=0
tehas_fixer_why=""
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_fixer_grant() { # <issue> <body sha256>
  local record=$TEHAS_HOME/state/cycles/$1.approved.json had
  tehas_fixer_rests_on=""
  tehas_fixer_count=0
  tehas_fixer_why=""
  if [ "$1" = "${TEHAS_CONTROL_ISSUE:-}" ]; then
    tehas_fixer_why="it is the control issue"
    return 1
  fi
  if ! tehas_fixer_rests_on=$(tehas_fixer_base "$record"); then
    tehas_fixer_rests_on=""
    tehas_fixer_why="no person approved an earlier cycle of it, so the fixer may not start one"
    return 1
  fi
  if [ "$2" != "$(jq -r .body_sha256 <<<"$tehas_fixer_rests_on")" ]; then
    tehas_fixer_why="its text is not the text $(jq -r .by <<<"$tehas_fixer_rests_on") approved"
    return 1
  fi
  had=$(tehas_fixer_starts "$record") || had=""
  if ! [[ $had =~ ^[0-9]+$ ]]; then
    tehas_fixer_why="its approval record does not say how many cycles the fixer started"
    return 1
  fi
  if [ "$had" -ge "${TEHAS_FIXER_MAX_RETRIES:-2}" ]; then
    tehas_fixer_why="the fixer has started $had cycles on it since $(jq -r .by <<<"$tehas_fixer_rests_on") approved it, the limit"
    return 1
  fi
  tehas_fixer_count=$((had + 1))
}

tehas_cycle_history_add() { # <issue> <JSON object>
  local file=$TEHAS_HOME/state/cycles/$1.history.jsonl
  [ ! -L "$file" ] || return 1
  jq -ec 'objects' <<<"$2" >>"$file" 2>/dev/null || return 1
  [ "$(wc -l <"$file")" -le 50 ] && return 0
  tail -n 50 "$file" >"$file.new" && mv "$file.new" "$file"
}

tehas_cycle_history() { # <issue>
  local file=$TEHAS_HOME/state/cycles/$1.history.jsonl
  [ -f "$file" ] && [ ! -L "$file" ] || return 0
  jq -cR 'fromjson? | objects' "$file" 2>/dev/null || true
}

tehas_park_signature() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def park_signature: [.step, .cap, .park_reason, .failure_kind, .failure_cause] | map(. // "" | tostring) | join("|");'
}
