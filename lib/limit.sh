# shellcheck shell=bash
# The factory's limit: an outside cap on what it starts, set with tehas limit.
# One file, $TEHAS_HOME/state/limit.json, {v: 1, cycles, plan, until, reason},
# written whole by tehas limit and read here by everything that honours it:
# the dispatcher on every poll, tehasbot improve before a run, and tehasbot
# health for tehas status.
#
#   tehas_limit_file               the file's path
#   tehas_limit_read [<now>]       the limit as one compact object on standard
#                                  output, at <now> epoch seconds or now:
#                                  {state: "none"} when no limit is set;
#                                  {state: "ok" or "expired", cycles, plan,
#                                  until, reason}, until and reason left out
#                                  when not set, expired once until has passed;
#                                  {state: "unreadable"} when something is
#                                  there that is not a limit of version 1.
#                                  Never fails: what cannot be read is never
#                                  read as no limit.
#   tehas_limit_holds <limit json> 0 when it holds every start: expired or
#                                  unreadable
#   tehas_limit_line <limit json>  the line tehas limit and tehas status print
#
# Needs lib/status.sh for the one-line rule of a reason. Reads one file and
# writes nothing.

tehas_limit_file() { printf '%s/state/limit.json\n' "$TEHAS_PROJECT_HOME"; }

tehas_limit_read() { # [<now epoch seconds>]
  local file now=${1:-} json
  [ -n "$now" ] || now=$(date +%s)
  file=$(tehas_limit_file)
  if [ ! -e "$file" ] && [ ! -L "$file" ]; then
    echo '{"state":"none"}'
    return 0
  fi
  # shellcheck disable=SC2154  # lib/status.sh, which every caller sources first
  json=$(jq -ce --argjson now "$now" '
    select(type == "object" and .v == 1
      and (.cycles | type) == "number" and .cycles >= 0 and (.cycles | floor) == .cycles
      and (.plan | type) == "boolean"
      and (.until == null or (.until | type) == "string")
      and ((.reason // "") | type) == "string")
    | (if .until == null then null else (.until | fromdateiso8601) end) as $u
    | ((.reason // "") | '"$tehas_pause_oneline"' | .[:200]) as $reason
    | {state: (if $u != null and $u <= $now then "expired" else "ok" end), cycles, plan}
      + (if .until == null then {} else {until} end)
      + (if $reason == "" then {} else {reason: $reason} end)' "$file" 2>/dev/null) || json='{"state":"unreadable"}'
  printf '%s\n' "$json"
}

tehas_limit_holds() { # <limit json>
  [ "$(jq -r .state <<<"$1")" != ok ] && [ "$(jq -r .state <<<"$1")" != none ]
}

# The words are lib/status.sh's, where tehas status and tehas top read them.
tehas_limit_line() { # <limit json>
  # shellcheck disable=SC2154  # lib/status.sh, which every caller sources first
  jq -r "$tehas_state_defs"' limit_line' <<<"$1"
}
