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
# The reading, the checks, the write and the words of the answer are the
# console's, in console/src/flags.rs: this file is the shell's name for them.
# Where no console can be run, tehas_limit_reading and tehas_limit_line answer.

tehas_limit_file() { printf '%s/state/limit.json\n' "$TEHAS_HOME"; }

tehas_limit_console_of() { # the console of this run, or nothing when none can
# be run: TEHAS_CONSOLE, or the one beside the commands which source this file.
  local c=${TEHAS_CONSOLE:-${tehas_limit_home:-$(
    cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}/libexec/console}
  [ -x "$c" ] && printf '%s\n' "$c"
}

tehas_limit_reading() { # <file> <now epoch seconds>: the shape of the
# console's `flags limit read`, in jq alone. What cannot be read is never read
# as no limit, so a reader holds every start rather than starting what a broken
# file hides.
  local file=$1 now=$2 json
  { [ -e "$file" ] || [ -L "$file" ]; } || { printf '%s\n' '{"state":"none"}'; return; }
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

tehas_limit_read() { # [<now epoch seconds>]: of the console when one can be
# run, of the file when none can, which is a test and not a host.
  local c now=${1:-$EPOCHSECONDS}
  c=$(tehas_limit_console_of)
  [ -n "$c" ] && { "$c" flags limit read --now "$now"; return; }
  tehas_limit_reading "$(tehas_limit_file)" "$now"
}

tehas_limit_holds() { # <limit json>
  [ "$(jq -r .state <<<"$1")" != ok ] && [ "$(jq -r .state <<<"$1")" != none ]
}

# The words of tehas status (console/src/status.rs), except for a limit in
# force with a console to ask, which prints its own.
tehas_limit_line() { # <limit json>
  local c
  c=$(tehas_limit_console_of)
  [ "$(jq -r '.state // "?"' <<<"$1" 2>/dev/null)" != ok ] || [ -z "$c" ] ||
    { "$c" flags limit show; return; }
  # shellcheck disable=SC2154  # lib/status.sh, which every caller sources first
  jq -r '"limit     " + (
      if .state == "none" then "none"
      elif .state == "unreadable" then "unreadable: nothing starts until it is set again or cleared"
      elif .state == "expired" then "expired at \(.until): nothing starts until it is set again or cleared"
      else "\(.cycles) cycle\(if .cycles == 1 then "" else "s" end) at once, planning \(if .plan then "on" else "off" end), "
        + (if .until == null then "until cleared" else "until \(.until)" end)
        + (if .reason == null then "" else ": \(.reason | '"$tehas_pause_oneline"')" end)
      end)' <<<"$1"
}
