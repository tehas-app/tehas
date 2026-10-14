# shellcheck shell=bash
# The factory's state as one file writes it: $TEHAS_HOME/live/health.json, the
# health file tehasbot health refreshes at every poll and at the end of a cycle.
#
# `tehas status` prints these lines and `tehas top` puts them in a frame. The
# words and the ten-minute rule live here once, so the two cannot drift apart.
#
#   tehas_health_max_age            the age after which the file is not believed
#   tehas_health_readable <file>    0 when the file is there and holds a status
#   tehas_health_stale <file> <now> 0 when the file is older than the rule
#                                   (a time that cannot be read counts as old)
#   tehas_state_lines <json> <now> <stale>   the factory line and the queue line
#   tehas_cycle_line <json> <now>            one cycle, as health.json writes it
#   tehas_slice_line <health json> <now>     the last poll's slice check
#   tehas_platform_line <health json> <now>  the platform checkout against
#                                            origin; nothing without one
#
# <json> is JSON text given as an argument: nothing here opens a file, loads the
# instance settings or writes anything.

# shellcheck disable=SC2016  # the jq program below is data, not a shell string
tehas_health_max_age=600

# A health file with a status string in it.
tehas_health_readable() { # <file>
  jq -e '.status | type == "string"' "$1" >/dev/null 2>&1
}

tehas_health_stale() { # <file> <now epoch seconds>
  local written
  written=$(jq -r '.time | try fromdateiso8601 catch 0' "$1" 2>/dev/null) || written=0
  [ $(( $2 - written )) -le "$tehas_health_max_age" ]
}

# The age of something, in the words tehas status has always used.
tehas_state_defs='
  def ago($now): ($now - .) | if . < 0 then 0 else . end
    | if . < 120 then "\(.) s ago" elif . < 7200 then "\(. / 60 | floor) min ago" else "\(. / 3600 | floor) h ago" end;
  def age($now): (try (fromdateiso8601 | ago($now)) catch "at an unknown time");
'

# The two lines a health object gives: is the factory taking work, and what is
# in the queue.
tehas_state_lines() { # <health json> <now epoch seconds> <stale true|false>
  jq -r --argjson now "$2" --argjson stale "$3" "$tehas_state_defs"'
    (if $stale then "factory   stale: the dispatcher last reported \(.time | age($now)), is its timer running? It said \(.status): \(.output // "")"
     else "factory   \(.status): \(.output // "") (\(.time | age($now)))" end),
    (.checks.queue // null | if . == null then "queue     no snapshot yet"
     else "queue     \(.inLine) in line, \(.building) building, \(.review) in review, \(.stopped) stopped"
          + (if .paused then ", paused" else "" end) + " (\(.time // "" | age($now)))" end)
  ' <<<"$1"
}

# One cycle as health.json writes it: issue, running, status, step, startedAt,
# time. null is a factory with no cycle yet.
tehas_cycle_line() { # <cycle json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs"'
    if . == null then "cycle     none yet"
    elif .running then "cycle     #\(.issue) running, step \(.step // "-"), started \(.startedAt // "" | age($now))"
    else "cycle     #\(.issue) \(.status // "unknown") at step \(.step // "-"), \(.time // "" | age($now))" end
  ' <<<"$1"
}

# The slice status captured by the dispatcher's last poll.
tehas_slice_line() { # <health json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs"'
    def gib: (. // 0) / 1073741824 * 10 | round / 10;
    (.checks.slice // null
     | if . == null then "slice     unknown: no poll has reported in 10 min"
       elif (.why // "") | IN("", "resources") | not then "slice     \(.hold // "holding") (\(.time | age($now)))"
       else "slice     room for \(.room // 0) cycles (\(.committed | gib) GiB of \(.max | gib) GiB promised, reserve \(.reserve // "unknown"), \(.time | age($now)))" end)
  ' <<<"$1"
}

# The platform checkout as tehasbot status last read it.
tehas_platform_line() { # <health json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs"'
    .checks.platform // empty
    | "platform  \((.commit // "unknown")[:7]), "
      + (if .behind > 0 then "\(.behind) commits behind \(.upstream) since \(.behindSince // "" | .[:10])"
         else "up to date with \(.upstream)" end)
      + (if .fetchedAt == null then ", never fetched" else ", fetched \(.fetchedAt | age($now))" end)
  ' <<<"$1"
}
