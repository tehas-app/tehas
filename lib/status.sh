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
#   tehas_state_lines <json> <now> <stale>   the factory line, the queue line,
#                                            and a line for each pause switch
#   tehas_cycle_line <json> <now>            one cycle, as health.json writes it
#   tehas_plan_lines <health json> <now>     a line for each planning turn in
#                                            flight; nothing without one
#   tehas_slice_line <health json> <now>     the last poll's slice check
#   tehas_platform_line <health json> <now>  the platform the host runs (its
#                                            release, or its checkout against
#                                            origin) as one line; nothing
#                                            without either
#   tehas_status_json <health json> <now> <stale>
#                                            the object of tehas status --json
#
# And the pause flag itself, which every command that reads it reads through:
#
#   tehas_pause_read <file>       the flag tehas pause writes, {by, at, reason}
#
# <json> is JSON text given as an argument: nothing here loads the instance
# settings or writes anything. tehas_pause_read is the one function that reads a
# file, and it reads only the one its argument names.
#
# The jq behind the state lines and the cycle line is named - tehas_state_line_jq
# and tehas_cycle_line_jq - because `tehas top` draws a whole frame in one jq
# pass and needs those two programs inside it. The words stay here either way:
# a frame that copied them would drift from these lines.

# shellcheck disable=SC2016  # the jq program below is data, not a shell string
tehas_health_max_age=600

# A pause reason is printed as one line of tehas status and written to files a
# page may read, so it holds no line of its own: every control character (tab,
# carriage return, escape, the separators U+2028 and U+2029) becomes a space,
# and the ends are trimmed. The reader applies it and the line writer applies
# it again, so a reason that reached a file some other way forges no line.
tehas_pause_oneline='gsub("[\u0001-\u001f\u007f-\u009f\u2028\u2029]+"; " ") | gsub("^ *| *$"; "")'

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
  # The triple of vX.Y.Z, or null for anything else: a version that cannot be
  # read as a triple is never said to be newer than another.
  def triple: (try (ltrimstr("v") | split(".") | map(tonumber)) catch null);
  # 0 when the version string $a is newer than the version string $b; either
  # one that is no triple is never newer than the other.
  def newer($a; $b): ([$a, $b] | map(triple) | all(. != null))
    and (($a | triple) as $x | ($b | triple) as $y | ($x | length) as $n
      | [range($n)] | map($x[.] > $y[.]) | any(.));
  # What holds the factory paused, one line for each switch that is set:
  # {flag: {at, reason} or null, label: true|false, lifted: true|false}, as
  # health.json writes it. lifted names the case where the label is off and no
  # allowed account took it off, so the host stays paused: a stranger removed it,
  # or the label object itself was deleted.
  def oneline: '"$tehas_pause_oneline"';
  # The limit tehas limit sets, as lib/limit.sh reads it.
  def limit_line: "limit     " + (
      if .state == "none" then "none"
      elif .state == "unreadable" then "unreadable: nothing starts until it is set again or cleared"
      elif .state == "expired" then "expired at \(.until): nothing starts until it is set again or cleared"
      else "\(.cycles) cycle\(if .cycles == 1 then "" else "s" end) at once, planning \(if .plan then "on" else "off" end), "
        + (if .until == null then "until cleared" else "until \(.until)" end)
        + (if .reason == null then "" else ": \(.reason | oneline)" end)
      end);
  def pause_lines($now): if . == null or (.flag == null and .label != true and .lifted != true) then empty else . end
    | ((if .flag == null then empty
        else "pause     flag, written \(.flag.at // "" | age($now))"
             + (if (.flag.reason // "" | oneline) == "" then "" else ": \(.flag.reason | oneline)" end)
        end),
       (if .label == true then "pause     label: tehas:pause on the control issue" else empty end),
       (if .lifted == true then "pause     label: tehas:pause is off the control issue, but no account in TEHAS_ALLOWED_LABELERS took it off" else empty end));
'

# tehas_pause_read <file>: the pause flag, as tehas pause writes it.
#   0  <file> holds {by, at, reason}: the object, compact, on standard output
#   1  no such file: nothing on standard output
#   2  something is at <file> and is not a pause flag: nothing on standard output
# A flag that cannot be understood is never read as no flag: every caller counts
# 2 as paused, so a damaged flag stops the factory rather than starting it.
# `at` and `reason` must be strings; `by` may be anything and reads as nobody.
# Nothing that reaches a page publishes `by`: it names an account on this host.
# The reason comes back on one line, kept to the 200 characters tehas pause
# writes, whatever the file holds.
tehas_pause_read() { # <file>
  local json
  [ -e "$1" ] || return 1
  json=$(jq -ce '
    select(type == "object")
    | select((.at | type) == "string")
    | select((.reason | type) == "string")
    | {by: (if (.by | type) == "string" then .by else "" end), at,
       reason: (.reason | '"$tehas_pause_oneline"' | .[:200])}' "$1" 2>/dev/null) || return 2
  printf '%s\n' "$json"
}

# The lines a health object gives: is the factory taking work, what is in the
# queue, and what holds it paused. The program takes the health object as its
# input and reads the epoch seconds as $now and the ten-minute rule as $stale,
# which is also how the frame in lib/top.sh calls it.
tehas_state_line_jq='
    (if $stale then "factory   stale: the dispatcher last reported \(.time | age($now)), is its timer running? It said \(.status): \(.output // "")"
     else "factory   \(.status): \(.output // "") (\(.time | age($now)))" end),
    (.checks.queue // null | if . == null then "queue     no snapshot yet"
     else "queue     \(.inLine) in line, \(.building) building, \(.review) in review, \(.stopped) stopped"
          + (if .paused then ", paused" else "" end) + " (\(.time // "" | age($now)))" end),
    (.checks.pause // null | pause_lines($now)),
    (.checks.limit // null | if . == null or .state == "none" then empty else limit_line end),
    (.checks.demand.refreshHold // null | strings | "refresh   held: \(.)"),
    (if .checks.allHands == true then "all-hands tehas:all-hands is on the control issue, applied by an allowed account" else empty end)
  '

tehas_state_lines() { # <health json> <now epoch seconds> <stale true|false>
  jq -r --argjson now "$2" --argjson stale "$3" "$tehas_state_defs$tehas_state_line_jq" <<<"$1"
}

# One cycle as health.json writes it: issue, running, status, step, startedAt,
# time. null is a factory with no cycle yet.
tehas_cycle_line_jq='
    if . == null then "cycle     none yet"
    elif .running then "cycle     #\(.issue) running, step \(.step // "-"), started \(.startedAt // "" | age($now))"
    else "cycle     #\(.issue) \(.status // "unknown") at step \(.step // "-"), \(.time // "" | age($now))" end
  '

tehas_cycle_line() { # <cycle json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs$tehas_cycle_line_jq" <<<"$1"
}

# The planning turns in flight, as health.json lists them: issue, turn,
# startedAt.
tehas_plan_lines() { # <health json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs"'
    (.checks.plans // [])[]
    | "plan      #\(.issue) turn \(.turn) running, started \(.startedAt // "" | age($now))"
  ' <<<"$1"
}

# The slice status captured by the dispatcher's last poll.
tehas_slice_line() { # <health json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs"'
    def gib: (. // 0) / 1073741824 * 10 | round / 10;
    (.checks.slice // null
     | if . == null then "slice     unknown: no poll has reported in 10 min"
       elif (.why // "") | IN("", "resources") | not then "slice     \(.hold // "holding") (\(.time | age($now)))"
       else "slice     room for \(.room // 0) cycles (\(.committed | gib) GiB of \(.max | gib) GiB promised, reserve \(.reserve // "unknown")\(if .refreshRoom == null then "" else ", refresh lane \(.refreshRoom) free" end), \(.time | age($now)))" end)
  ' <<<"$1"
}

# The platform the host runs, as tehasbot status last read it: the release of
# the tree under ~/tehas/current, or the checkout under ~/tehas/platform
# against origin.
# platform() in libexec/status writes it: {commit, upstream, behind,
# behindSince, fetchedAt} for the checkout, or {release, newest, behind: 0,
# upstream: "releases", fetchedAt} for the release a host runs through
# ~/tehas/current.
tehas_platform_line() { # <health json> <now epoch seconds>
  jq -r --argjson now "$2" "$tehas_state_defs"'
    .checks.platform // empty
    | if .release != null then
        "platform  \(.release), "
        + (if .newest == null then "newest unknown"
           elif newer(.newest; .release) then "\(.newest) is out"
           else "newest \(.newest)" end)
        + (if .fetchedAt == null then ", never fetched" else ", fetched \(.fetchedAt | age($now))" end)
      else
        "platform  \((.commit // "unknown")[:7]), "
        + (if .behind > 0 then "\(.behind) commits behind \(.upstream) since \(.behindSince // "" | .[:10])"
           else "up to date with \(.upstream)" end)
        + (if .fetchedAt == null then ", never fetched" else ", fetched \(.fetchedAt | age($now))" end)
      end
  ' <<<"$1"
}

# The object tehas status --json prints, from the health object. A field
# without a value is left out, never null; host/docs/api.md has the rest.
tehas_status_json() { # <health json> <now epoch seconds> <stale true|false>
  jq -c --argjson now "$2" --argjson stale "$3" '
    def since: try (fromdateiso8601 | $now - . | if . < 0 then 0 else . end) catch null;
    def oneline: '"$tehas_pause_oneline"';
    (.checks.pause // {}) as $p
    | (.checks.slice // null) as $s
    | {api: 1, time, age: (.time | since), stale: $stale, state: .status,
       reasons: (if .status == "pass" then [] else [.output // ""] end),
       paused: {local: ($p.flag != null), label: ($p.label == true or $p.lifted == true)},
       allHands: .checks.allHands,
       limit: (.checks.limit // null | if . == null then null else del(.reason) end),
       running: [(.checks.cycles // [])[] | {issue, since: .startedAt} + (if .step == null then {} else {step} end)],
       planning: [(.checks.plans // [])[] | {issue, turn} + (if .startedAt == null then {} else {since: .startedAt} end)],
       demand: (.checks.demand // null
                | if . == null then null
                  else {waiting} + (if .oldest == null then {} else {oldestWait: (.oldest | since)} end)
                       + (if .refreshHold == null then {} else {refreshHold: (.refreshHold | oneline)} end) end),
       hold: (if $p.flag != null then
                {why: "pause", text: ("paused by the flag tehas pause wrote"
                  + (if ($p.flag.reason // "" | oneline) == "" then "" else ": \($p.flag.reason | oneline)" end))}
              elif $p.label == true then {why: "pause", text: "paused by tehas:pause on the control issue"}
              elif $p.lifted == true then {why: "pause", text: "paused: tehas:pause was taken off by an account that may not lift it"}
              elif $s != null and ($s.why // "") != "" then {why: $s.why, text: ($s.hold // "")}
              else null end),
       room: (if $s == null then null
              else {cycles: $s.free, refreshes: $s.refreshRoom, maxAwaiting: $s.awaitRoom, slice: $s.room, limit: $s.limitRoom}
                   | with_entries(select(.value != null)) end)}
    | with_entries(select((.key | IN("allHands", "limit") | not) or .value != null))
  ' <<<"$1"
}
